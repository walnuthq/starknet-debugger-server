use internal_tracing::{
    call_trace::{DebuggerTraceEntry, DebuggerTraceEntryWithContractCall},
    function_calls_map::FunctionCallsMap,
};

use crate::contract_calls_map::ContractCallsMap;

pub fn merge_debugger_trace(
    contract_call_id: &u32,
    function_calls_map: &mut FunctionCallsMap,
    contract_calls_map: &ContractCallsMap,
) -> Vec<DebuggerTraceEntry> {
    let mut trace: Vec<DebuggerTraceEntry> = Vec::new();
    let mut no_trace_reason: Option<String> = None;

    if let Some(contract_call) = contract_calls_map.0.get(contract_call_id) {
        if let Some(call_debugger_data) = &contract_call.call_debugger_data {
            if call_debugger_data.execution_trace.len() > 0 {
                for step in call_debugger_data.execution_trace.iter() {
                    match step {
                        DebuggerTraceEntry::WithLocation(with_location) => {
                            let function_call = function_calls_map
                                .0
                                .get_mut(&with_location.function_call_id)
                                .unwrap();

                            if function_call.debugger_trace_step_index.is_none() {
                                function_call.debugger_trace_step_index = Some(trace.len());
                            }

                            trace.push(DebuggerTraceEntry::WithLocation(with_location.clone()));
                        }
                        DebuggerTraceEntry::WithContractCall(with_contract_call) => {
                            trace.extend(merge_debugger_trace(
                                &with_contract_call.contract_call_id,
                                function_calls_map,
                                contract_calls_map,
                            ));
                        }
                    }
                }
            } else {
                no_trace_reason = Some("Execution trace is empty".to_string());
            }
        } else {
            no_trace_reason = Some("No call debugger data available".to_string());
        }
    } else {
        no_trace_reason = Some("Contract call not found".to_string());
    }

    if no_trace_reason.is_some() {
        trace.push(DebuggerTraceEntry::WithContractCall(
            DebuggerTraceEntryWithContractCall {
                contract_call_id: *contract_call_id,
                reason: Some(no_trace_reason.unwrap()),
            },
        ));

        for child_contract_call_id in contract_calls_map
            .0
            .get(contract_call_id)
            .unwrap()
            .children_call_ids
            .iter()
        {
            trace.extend(merge_debugger_trace(
                child_contract_call_id,
                function_calls_map,
                contract_calls_map,
            ));
        }
    }

    trace
}
