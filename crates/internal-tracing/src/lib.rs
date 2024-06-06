use anyhow::{Context, Error};
use byteorder::{ByteOrder, LittleEndian};
use cairo_felt::{Felt252, PRIME_STR};
use cairo_lang_sierra::{
    extensions::{
        core::{CoreLibfunc, CoreType},
        gas::CostTokenType,
    },
    ids::ConcreteTypeId,
    program::{GenFunction, Program, StatementIdx},
    program_registry::ProgramRegistry,
};
use cairo_lang_sierra_to_casm::{
    compiler::{CairoProgram, CairoProgramDebugInfo, SierraToCasmConfig},
    metadata::{calc_metadata, MetadataComputationConfig},
};
use cairo_lang_sierra_type_size::{get_type_size_map, TypeSizeMap};
use cairo_lang_starknet_classes::{
    casm_contract_class::ENTRY_POINT_COST, contract_class::ContractClass,
    felt252_serde::sierra_from_felt252s,
};
use cairo_vm::{
    types::instruction::{Instruction, Op1Addr},
    vm::{decoding::decoder::decode_instruction, trace::trace_entry::TraceEntry},
};
use indextree::{Arena, NodeId};
use itertools::chain;
use num_bigint::{BigInt, BigUint};
use serde::Serialize;
use serde_json::Value;
use smol_str::SmolStr;
use starknet_api::core::ClassHash;
use std::{
    collections::{BTreeMap, HashMap},
    fs::File,
    path::{Path, PathBuf},
};

pub fn get_internal_fn_call_trace(
    class_hash: ClassHash,
    relocated_memory: &Vec<Option<Felt252>>,
    vm_trace: &Vec<TraceEntry>,
) -> Option<InternalFnCallTraceEntryNode> {
    let folder_with_precompiled_contracts = "precompiled-contracts";
    let file_name_with_sierra_contract = format!("{}.json", class_hash.to_string());
    let file_path_with_sierra_contract =
        Path::new(folder_with_precompiled_contracts).join(file_name_with_sierra_contract);
    if file_path_with_sierra_contract.exists() {
        // Parse Sierra contract class
        let sierra_json = read_json(file_path_with_sierra_contract)
            .expect("Unable to read Sierra contract json file");
        let sierra_class: ContractClass = serde_json::from_value(sierra_json).unwrap();

        // Extract Sierra program
        let sierra_program = sierra_class.extract_sierra_program().unwrap();

        let type_names = sierra_class
            .sierra_program_debug_info
            .clone()
            .unwrap()
            .type_names;

        // Compile Sierra contract class to CASM Program
        let casm_program = compile_sierra_contract_class(sierra_class, usize::MAX);

        let casm_to_sierra_map = make_casm_to_sierra_map(&casm_program.debug_info, 0);

        let (pc_inst_map, pc_to_inst_indexes_map) = get_pc_mappings(relocated_memory, vm_trace);

        let memory_map: HashMap<usize, BigInt> = relocated_memory
            .iter()
            .filter_map(|x| x.as_ref().map(|_| x.clone().unwrap()))
            .map(|x| x.to_bigint())
            .enumerate()
            .map(|(i, v)| (i + 1, v))
            .collect();

        let sierra_program_registry: ProgramRegistry<CoreType, CoreLibfunc> =
            ProgramRegistry::<CoreType, CoreLibfunc>::new(&sierra_program).unwrap();
        let type_sizes =
            get_type_size_map(&sierra_program, &sierra_program_registry).unwrap_or_default();

        // Print relocated memory
        let mut ordered_map: BTreeMap<usize, BigInt> = BTreeMap::new();
        for (k, v) in &memory_map {
            ordered_map.insert(*k, v.clone());
        }

        for (k, v) in &ordered_map {
            println!("{}: {}", k, v);
        }
        // Print VM trace
        dbg!(&vm_trace);

        Some(get_internal_fn_calls_trace(
            vm_trace,
            pc_to_inst_indexes_map,
            casm_to_sierra_map,
            &sierra_program,
            memory_map,
            type_sizes,
            type_names,
        ))
    } else {
        println!(
            "Can't find Sierra contract for class hash: {:#?}",
            class_hash.to_string()
        );
        None
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct InternalFnCallTraceEntry {
    pub fn_name: Option<String>,
    pub fp: usize,
    pub results: Vec<InternalFnCallIO>,
    pub arguments: Vec<InternalFnCallIO>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InternalFnCallIO {
    pub type_name: String,
    pub type_size: i16,
    pub value: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InternalFnCallTraceEntryNode {
    pub data: InternalFnCallTraceEntry,
    pub nested_calls: Vec<InternalFnCallTraceEntryNode>,
}

#[derive(Debug)]
struct InternalFnCallTraceTree {
    arena: Arena<InternalFnCallTraceEntry>,
    current_node: NodeId,
    root: NodeId,
}

impl InternalFnCallTraceTree {
    fn new(root_entry: InternalFnCallTraceEntry) -> Self {
        let mut arena = Arena::new();
        let root = arena.new_node(root_entry);
        InternalFnCallTraceTree {
            arena,
            current_node: root,
            root,
        }
    }

    fn add_child(&mut self, entry: InternalFnCallTraceEntry) {
        let child = self.arena.new_node(entry);
        self.current_node.append(child, &mut self.arena);
        self.current_node = child;
    }

    fn move_to_parent(&mut self) {
        let mut ancestors = self.current_node.ancestors(&self.arena);
        ancestors.next();
        if let Some(parent) = ancestors.next() {
            self.current_node = parent;
        }
    }

    fn set_result_values_to_current_node(&mut self, result_values: Vec<Vec<String>>) {
        if let Some(node) = self.arena.get_mut(self.current_node) {
            let data = node.get_mut();
            for (i, result) in data.results.iter_mut().enumerate() {
                result.value = result_values[i].clone();
            }
        }
    }

    fn get_serializable(&self, node_id: NodeId) -> InternalFnCallTraceEntryNode {
        let mut nested_calls = Vec::new();
        for child_node_id in node_id.children(&self.arena) {
            nested_calls.push(self.get_serializable(child_node_id));
        }
        InternalFnCallTraceEntryNode {
            data: self.arena[node_id].get().clone(),
            nested_calls,
        }
    }

    fn get_root_serializable(&self) -> InternalFnCallTraceEntryNode {
        self.get_serializable(self.root)
    }

    fn get_node_data(&self, node_id: NodeId) -> &InternalFnCallTraceEntry {
        &self.arena[node_id].get()
    }
}

fn get_internal_fn_calls_trace(
    vm_trace: &Vec<TraceEntry>,
    pc_to_inst_indexes_map: HashMap<usize, usize>,
    casm_to_sierra_map: HashMap<usize, Vec<usize>>,
    sierra_program: &Program,
    memory_map: HashMap<usize, BigInt>,
    type_sizes: TypeSizeMap,
    type_names: HashMap<ConcreteTypeId, SmolStr>,
) -> InternalFnCallTraceEntryNode {
    let first_vm_trace_entry = vm_trace.first().unwrap();
    let mut current_fp = first_vm_trace_entry.fp;

    let entrypoint_function = get_sierra_function_at_pc(
        &first_vm_trace_entry.pc,
        &pc_to_inst_indexes_map,
        &casm_to_sierra_map,
        sierra_program,
    );

    let mut tree = InternalFnCallTraceTree::new(create_internal_fn_call_trace_entry(
        entrypoint_function,
        current_fp,
        &type_sizes,
        &type_names,
    ));

    for trace_entry in vm_trace.iter() {
        let new_fp = trace_entry.fp;
        if new_fp > current_fp {
            // new function call
            let function = get_sierra_function_at_pc(
                // get sierra function at pc
                &trace_entry.pc,
                &pc_to_inst_indexes_map,
                &casm_to_sierra_map,
                sierra_program,
            );
            let mut call_entry =
                create_internal_fn_call_trace_entry(function, new_fp, &type_sizes, &type_names);

            // https://docs.cairo-lang.org/how_cairo_works/functions.html#argument
            // TODO: check if is algorithm is correct and fix if needed
            for argument in call_entry.arguments.iter_mut().rev() {
                let type_size = argument.type_size as usize;
                let mut values: Vec<String> = Vec::new();
                for i in 1..(type_size + 1) {
                    let addr = trace_entry.ap - 2 - type_size + i;
                    let value = memory_map.get(&addr).unwrap();
                    values.push(value.clone().to_string());
                }
                argument.value = values;
            }
            tree.add_child(call_entry);
            current_fp = trace_entry.fp;
        } else if new_fp < current_fp {
            // return from function
            let exit_function = tree.get_node_data(tree.current_node);
            let mut result_values: Vec<Vec<String>> = Vec::new();
            let mut offset = 0;

            // https://docs.cairo-lang.org/how_cairo_works/functions.html#return-values
            // TODO: check if is algorithm is correct and fix if needed
            for result in exit_function.results.iter().rev() {
                let type_size = result.type_size as usize;
                let mut values: Vec<String> = Vec::new();
                for i in 1..(type_size + 1) {
                    let addr = trace_entry.ap - type_size + i - offset;
                    let value = memory_map.get(&addr).unwrap();
                    values.push(value.clone().to_string());
                }
                result_values.push(values);
                offset += type_size;
            }
            result_values.reverse();
            tree.set_result_values_to_current_node(result_values);
            tree.move_to_parent();
            current_fp = trace_entry.fp;
        }
    }

    tree.get_root_serializable()
}

fn create_internal_fn_call_trace_entry(
    function: Option<&GenFunction<StatementIdx>>,
    fp: usize,
    type_sizes: &TypeSizeMap,
    type_names: &HashMap<ConcreteTypeId, SmolStr>,
) -> InternalFnCallTraceEntry {
    let mut results: Vec<InternalFnCallIO> = Vec::new();
    let mut arguments: Vec<InternalFnCallIO> = Vec::new();
    if let Some(function) = function {
        function.signature.ret_types.iter().for_each(|ret_type| {
            results.push(InternalFnCallIO {
                type_name: type_names.get(ret_type).unwrap().to_string(),
                type_size: *type_sizes.get(&ret_type).unwrap(),
                value: Vec::new(),
            });
        });
        function.params.iter().for_each(|param| {
            arguments.push(InternalFnCallIO {
                type_name: type_names.get(&param.ty).unwrap().to_string(),
                type_size: *type_sizes.get(&param.ty).unwrap(),
                value: Vec::new(),
            });
        });
    }
    InternalFnCallTraceEntry {
        fn_name: function.map(|f| f.to_string()),
        fp,
        results,
        arguments,
    }
}

fn get_sierra_function_at_pc<'a>(
    pc: &'a usize,
    pc_to_inst_indexes_map: &'a HashMap<usize, usize>,
    casm_to_sierra_map: &'a HashMap<usize, Vec<usize>>,
    sierra_program: &'a Program,
) -> Option<&'a GenFunction<StatementIdx>> {
    let casm_index = pc_to_inst_indexes_map
        .get(pc)
        .expect("Failed to get casm index");
    let sierra_indexes = casm_to_sierra_map.get(casm_index);
    if let Some(sierra_indexes) = sierra_indexes {
        let first_sierra_index = sierra_indexes.first();
        if let Some(first_sierra_index) = first_sierra_index {
            let func = sierra_program
                .funcs
                .iter()
                .find(|&func| func.entry_point.0 == *first_sierra_index);
            return func;
        }
    }
    return None;
}

fn get_pc_mappings(
    relocated_memory: &Vec<Option<Felt252>>,
    vm_trace: &Vec<TraceEntry>,
) -> (HashMap<usize, Instruction>, HashMap<usize, usize>) {
    let max_pc_entry = vm_trace.iter().max_by(|a, b| a.pc.cmp(&b.pc));

    let max_pc = match max_pc_entry {
        Some(max_entry) => max_entry.pc,
        None => {
            println!("No entries in the trace");
            0
        }
    };

    let mut pc_inst_map: HashMap<usize, Instruction> = HashMap::new();
    // let mut pc_inst_serialized_map: HashMap<usize, InstructionSerializable> = HashMap::new();
    let mut pc_to_inst_indexes_map: HashMap<usize, usize> = HashMap::new();

    let mut skip_next_pc = false;
    let mut casm_index: usize = 0;
    for pc in 1..=max_pc {
        if skip_next_pc {
            skip_next_pc = false;
            continue;
        }

        let (instruction_encoding_felt, _) = get_instruction_encoding(pc, &relocated_memory)
            .expect("Failed to get instruction encoding");
        let instruction_encoding_bytes_le = instruction_encoding_felt.to_le_bytes();
        let instruction_encoding_u64 = LittleEndian::read_u64(&instruction_encoding_bytes_le[..]);
        let instruction =
            decode_instruction(instruction_encoding_u64).expect("Failed to decode instruction");
        pc_inst_map.insert(pc, instruction.clone());
        if instruction.op1_addr == Op1Addr::Imm {
            skip_next_pc = true;
        }
        // pc_inst_serialized_map.insert(pc, InstructionSerializable(instruction));
        pc_to_inst_indexes_map.insert(pc, casm_index);
        casm_index += 1;
    }
    (pc_inst_map, pc_to_inst_indexes_map)
}

fn compile_sierra_contract_class(
    contract_class: ContractClass,
    max_bytecode_size: usize,
) -> CairoProgram {
    let (sierra_version, _, program) =
        sierra_from_felt252s(&contract_class.sierra_program).unwrap();

    let entrypoint_function_indices = chain!(
        &contract_class.entry_points_by_type.constructor,
        &contract_class.entry_points_by_type.external,
        &contract_class.entry_points_by_type.l1_handler,
    )
    .map(|entrypoint| entrypoint.function_idx);

    let entrypoint_ids = entrypoint_function_indices.map(|idx| program.funcs[idx].id.clone());

    let no_eq_solver = sierra_version.minor >= 4;

    let metadata_computation_config = MetadataComputationConfig {
        function_set_costs: entrypoint_ids
            .map(|id| (id, [(CostTokenType::Const, ENTRY_POINT_COST)].into()))
            .collect(),
        linear_gas_solver: no_eq_solver,
        linear_ap_change_solver: no_eq_solver,
        skip_non_linear_solver_comparisons: false,
        compute_runtime_costs: false,
    };

    let metadata = calc_metadata(&program, metadata_computation_config).unwrap();

    cairo_lang_sierra_to_casm::compiler::compile(
        &program,
        &metadata,
        SierraToCasmConfig {
            gas_usage_check: true,
            max_bytecode_size,
        },
    )
    .unwrap()
}

fn make_casm_to_sierra_map(
    debug_info: &CairoProgramDebugInfo,
    casm_headers_len: usize,
) -> HashMap<usize, Vec<usize>> {
    let mut map: HashMap<usize, Vec<usize>> = HashMap::new();
    let sierra_statement_info_len = debug_info.sierra_statement_info.len();
    for (i, sierra_info) in debug_info
        .sierra_statement_info
        .iter()
        .enumerate()
        .take(sierra_statement_info_len - 1)
    {
        let key = sierra_info.instruction_idx + casm_headers_len;
        map.entry(key).or_insert_with(Vec::new).push(i);
    }
    map
}

// Returns the encoded instruction (the value at pc) and the immediate value (the value at
// pc + 1, if it exists in the memory).
pub fn get_instruction_encoding(
    pc: usize,
    memory: &[Option<Felt252>],
) -> anyhow::Result<(Felt252, Option<Felt252>)> {
    if memory[pc].is_none() {
        return Err(Error::msg("Memory at pc is None"));
    }
    let instruction_encoding = memory[pc].clone().unwrap();
    let prime = BigUint::parse_bytes(PRIME_STR[2..].as_bytes(), 16).unwrap();

    let imm_addr = BigUint::from(pc + 1) % prime;
    let imm_addr = usize::try_from(imm_addr.clone()).map_err(|_| Error::msg(""))?;
    let optional_imm = memory[imm_addr].clone();
    Ok((instruction_encoding, optional_imm))
}

#[derive(Serialize, Debug)]
pub struct SierraFormattedProgram {
    pub type_declarations: Vec<String>,
    pub libfunc_declarations: Vec<String>,
    pub statements: Vec<String>,
    pub funcs: Vec<String>,
}

fn format_sierra_program(sierra_program: &Program) -> SierraFormattedProgram {
    SierraFormattedProgram {
        type_declarations: sierra_program
            .type_declarations
            .iter()
            .map(|type_decl| type_decl.to_string())
            .collect(),
        libfunc_declarations: sierra_program
            .libfunc_declarations
            .iter()
            .map(|libfunc_decl| libfunc_decl.to_string())
            .collect(),
        statements: sierra_program
            .statements
            .iter()
            .enumerate()
            .map(|(index, statement)| format!("{} // {}", statement.to_string(), index))
            .collect(),
        funcs: sierra_program
            .funcs
            .iter()
            .map(|func| func.to_string())
            .collect(),
    }
}

fn read_json(file_path: PathBuf) -> anyhow::Result<Value> {
    let sierra_file = File::open(file_path).context("Unable to open json file")?;
    serde_json::from_reader(sierra_file).context("Unable to read json file")
}
