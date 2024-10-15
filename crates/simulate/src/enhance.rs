use internal_tracing::function_calls_map::FunctionCallsMap;
use internal_tracing::get_internal_trace_and_debugger_data;
use internal_tracing::ClassDebuggerDataWithContractClass;
use std::collections::HashMap;
use tracing::error;

use crate::contract_call::ContractCall;

pub fn enhance_call_tree_with_function_calls(
    contract_calls_map: &mut HashMap<u32, ContractCall>,
    next_call_id: u32,
    classes_debugger_data: &HashMap<String, ClassDebuggerDataWithContractClass>,
) -> FunctionCallsMap {
    let mut next_call_id = next_call_id;
    let mut function_calls_map = FunctionCallsMap::new();

    for (id, call) in contract_calls_map.iter_mut() {
        let result = match (&call.class_hash, &call.vm_memory, &call.vm_trace) {
            (Some(class_hash), Some(vm_memory), Some(vm_trace)) => {
                match classes_debugger_data.get(class_hash) {
                    Some(full_class_debugger_data) => {
                        match get_internal_trace_and_debugger_data(
                            vm_memory,
                            vm_trace,
                            full_class_debugger_data,
                            &mut function_calls_map,
                            &mut next_call_id,
                            *id,
                            &call.children_call_ids,
                        ) {
                            Ok((call_debugger_data, root_function_call_id)) => {
                                Some((call_debugger_data, root_function_call_id))
                            }
                            Err(e) => {
                                error!("Failed to get internal fn call trace: {:?}", e);
                                None
                            }
                        }
                    }
                    None => None,
                }
            }
            _ => {
                error!("Not enough data to get internal fn call trace");
                None
            }
        };

        if let Some((call_debugger_data, root_function_call_id)) = result {
            call.call_debugger_data = Some(call_debugger_data);
            call.function_call_id = Some(root_function_call_id);
            call.code_location = function_calls_map.0.get(&root_function_call_id).and_then(|call| call.code_location.clone());
        }

        call.vm_trace = None;
        call.vm_memory = None;
    }

    function_calls_map
}
