pub mod call_trace;
pub mod debugger_data_fetcher;
pub mod function_call;
pub mod function_calls_map;
pub mod mappings;
pub mod utils;

use anyhow::Result;
use cairo_lang_starknet_classes::contract_class::ContractClass;
use cairo_vm::vm::trace::trace_entry::RelocatedTraceEntry;
use call_trace::{get_internal_call_trace, DebuggerTraceEntry};
use function_calls_map::FunctionCallsMap;
use serde::Serialize;
use starknet::core::types::Felt;
use std::collections::HashMap;
use verification::SierraStatementToCairoDebugInfo;

use crate::mappings::Mappings;

/// Contains the debugger data for all classes in a simulation
#[derive(Debug, Serialize)]
pub struct SimulationDebuggerData {
    pub classes_debugger_data: HashMap<String, ClassDebuggerData>,
    pub debugger_trace: Vec<DebuggerTraceEntry>,
}

/// Contains the debugger data for a class with the Sierra contract class
#[derive(Debug)]
pub struct ClassDebuggerDataWithContractClass {
    pub class_debugger_data: Option<ClassDebuggerData>,
    pub contract_class: ContractClass,
}

/// Contains the debugger data for a class
#[derive(Debug, Serialize)]
pub struct ClassDebuggerData {
    pub sierra_statements_to_cairo_info: HashMap<usize, SierraStatementToCairoDebugInfo>,
    pub source_code: HashMap<String, String>,
}

/// Contains the debugger data for a contract call
#[derive(Debug, Serialize)]
pub struct ContractCallDebuggerData {
    pub execution_trace: Vec<DebuggerTraceEntry>,
}

pub fn debugger_data_maps_full_class_to_class(
    full_class_debugger_data_map: HashMap<String, ClassDebuggerDataWithContractClass>,
) -> HashMap<String, ClassDebuggerData> {
    full_class_debugger_data_map
        .into_iter()
        .filter_map(|(key, value)| {
            value
                .class_debugger_data
                .map(|class_debugger_data| (key, class_debugger_data))
        })
        .collect()
}

/// Returns the internal function call trace and sierra_execution_trace
pub fn get_internal_trace_and_debugger_data(
    vm_memory: &Vec<Option<Felt>>,
    vm_trace: &Vec<RelocatedTraceEntry>,
    full_class_debugger_data: &ClassDebuggerDataWithContractClass,
    function_calls_map: &mut FunctionCallsMap,
    next_call_id: &mut u32,
    contract_call_id: u32,
    contract_call_children_ids: &Vec<u32>,
) -> Result<(ContractCallDebuggerData, u32)> {
    let mappings = Mappings::new(
        vm_memory,
        vm_trace,
        full_class_debugger_data.contract_class.clone(),
    )?;

    let (execution_trace, root_function_call_id) = get_internal_call_trace(
        &mappings,
        vm_memory,
        vm_trace,
        full_class_debugger_data
            .class_debugger_data
            .as_ref()
            .map(|class_debugger_data| &class_debugger_data.sierra_statements_to_cairo_info),
        function_calls_map,
        next_call_id,
        contract_call_id,
        contract_call_children_ids,
    )?;

    Ok((
        ContractCallDebuggerData { execution_trace },
        root_function_call_id,
    ))
}
