

use crate::linear::{
    LinearConfig,
    client::LinearClient,
};

use crate::app::{ init_workflow_states, ALL_WORKFLOW_STATES, Platform };

use super::error::LinearClientError;

use tokio::runtime::Handle;


use serde_json::{ Value, Map };

use crate::linear::types::{ CustomView };

use crate::util::GraphQLCursor;


#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub enum FilterType {

    SelectedTeam,
    AllTeams,

    // Only one Content filter per view
    Content,

    SelectedState,
    SelectedCreator,
    SelectedLabel,
    SelectedAssignee,
    SelectedProject,
    SelectedPriority,
    SelectedSubscriber,

    DueToday,
    Overdue,
    HasDueDate,
    DueSoon,
    NoDueDate,

    NoLabel,
    NoAssignee,
    NoProject,
}

#[derive(Debug, PartialEq, Clone)]
pub struct Filter {
    pub filter_type: FilterType,
    pub ref_id: Option<String>,
}


pub async fn single_endpoint_fetch (  view_cursor: &mut GraphQLCursor,
    request_num: &mut u32,
    filter_data: &mut Value,
    linear_config: &LinearConfig,
) -> Vec<Value> {

    // Determine if filter_data has a state filter
    // If so: replace with case-sensitive names

    debug!("single_endpoint_fetch - received filter_data: {:?}", filter_data);
    
    let mut state_filter_opt: Option<usize> = None;
    if let Value::Array(filter_list) = &filter_data["and"] {
        state_filter_opt = filter_list.iter()
            .position(|filter_obj| {
                if let Some(filter_map) = filter_obj.as_object() {
                    filter_map.contains_key("state")
                } else {
                    false
                }
            })
    }
    if let Some(state_filter_idx) = state_filter_opt {

        let state_filter = &filter_data["and"][state_filter_idx]["state"]
            .as_object()
            .unwrap_or_else(|| panic!("filter_data[\"and\"][{:?}][\"state\"] is not a map", state_filter_idx));

        debug!("single_endpoint_fetch - attempting to replace state filter: {:?}", state_filter);
        
        // determine whether an inclusive or exclusive list
        // and fetch list of workflow state names used in filter
        let mut in_filter: bool = false;
        let mut ex_filter: bool = false;
        let mut state_list: Vec<String> = Vec::new();

        if let Value::Array(temp_state_list) = &state_filter["name"]["in"] {
            in_filter = true;
            state_list = temp_state_list.iter().map(|x| x.as_str().unwrap().to_string()).collect();
        } else if let Value::Array(temp_state_list) = &state_filter["name"]["nin"] {
            ex_filter = true;
            state_list = temp_state_list.iter().map(|x| x.as_str().unwrap().to_string()).collect();
        }

        let handle = Handle::current();

        let config_move = linear_config.clone();
        tokio::task::spawn_blocking(move || {
            init_workflow_states(config_move, handle);
        }).await.unwrap();
        debug!("EXTERIOR - init_workflow_states complete");

        let mut case_aware_states: Vec<String> = Vec::new();

        let all_workflow_states_lock = ALL_WORKFLOW_STATES.lock().unwrap();

        debug!("ALL_WORKFLOW_STATES: {:?}", *all_workflow_states_lock);
        debug!("state_list: {:?}", state_list);

        // do case-insensitive match against ALL_WORKFLOW_STATES
        for case_unaware_state in state_list.iter() {
            case_aware_states.push(
                all_workflow_states_lock
                    .iter()
                    .find(|case_aware_state| {
                        let case_aware_str = case_aware_state["name"].as_str().expect("state_obj with no name field").to_lowercase();
                        debug!("state comparison: {} == {}", case_unaware_state.to_lowercase(), case_aware_str);

                        case_unaware_state.to_lowercase() == case_aware_str
                    })
                    .expect("Could not find workflow state in ALL_WORKFLOW_STATES")
                    ["name"]
                    .as_str()
                    .unwrap()
                    .to_string()
            );
        }

        debug!("case_aware_states: {:?}", case_aware_states);

        // replace state filter with case-sensitive names
        if in_filter {
            filter_data["and"][state_filter_idx]["state"]["name"]["in"] = Value::Array(case_aware_states.iter()
                .map(|state_str| { Value::String(state_str.to_string()) })
                .collect()
            );
        } else if ex_filter {
            filter_data["and"][state_filter_idx]["state"]["name"]["nin"] = Value::Array(case_aware_states.iter()
                .map(|state_str| { Value::String(state_str.to_string()) })
                .collect()
            );
        }

        debug!("single_endpoint_fetch - replaced state filter: {:?}", filter_data["and"]["state"]);
    }


    let mut found_issue_list: Vec<Value> = Vec::new();

    let mut loop_num: u16 = 0;

    let mut query_result: Result<Value, LinearClientError>;
    let mut variables: Map<String, Value> = Map::new();

    loop {

        // If Query is exhausted
        if view_cursor.platform == Platform::Linear && !view_cursor.has_next_page {
            // No more Pages remaining, return found_issues_list
            debug!("Single Endpoint Fetch - no more issues to query, returning found_issues_list");
            return found_issue_list;
        }

        variables.insert(String::from("filterObj"), filter_data.clone());

        query_result = LinearClient::get_issues_by_filter_data(linear_config.clone(), Some(view_cursor.clone()), variables.clone()).await;

        if let Ok(response) = query_result {

            // Increment request_num here
            *request_num += 1;

            debug!("Current Filter Data Query Response: {:?}", response);

            // Filter returned Issues by all other loader filters
            // and add remainder to final_issue_list

            let mut returned_issues: Vec<Value> = match response["issue_nodes"].as_array() {
                Some(resp_issue_data) => {
                    resp_issue_data.clone()
                },
                None => {
                    error!("'issue_nodes' invalid format: {:?}", response["issue_nodes"]);
                    panic!("'issue_nodes' invalid format");
                }
            };

            debug!("returned_issues.len(): {:?}", returned_issues.len());

            if !returned_issues.is_empty() {
                found_issue_list.append(&mut returned_issues);
            }

            // Update GraphQLCursor
            match GraphQLCursor::linear_cursor_from_page_info(response["cursor_info"].clone()) {
                Some(new_cursor) => {
                    *view_cursor = new_cursor.clone();
                },
                None => {
                    error!("GraphQLCursor could not be created from response['cursor_info']: {:?}", response["cursor_info"]);
                    panic!("GraphQLCursor could not be created from response['cursor_info']: {:?}", response["cursor_info"]);
                },
            }
        }
        else {
            error!("View_Resolver Query Failed: {:?}", query_result);
            panic!("View_Resolver Query Failed: {:?}", query_result);
        }

        if found_issue_list.len() >= (linear_config.view_panel_page_size as usize)  {
            return found_issue_list;
        }

        info!("Loop {} - found_issue_list: {:?}", loop_num, found_issue_list);
        loop_num += 1;
    }
}

pub async fn optimized_view_issue_fetch (   view_obj: &CustomView,
                                            cursor_option: Option<GraphQLCursor>,
                                            linear_config: LinearConfig
                                        ) -> ( Vec<Value>, GraphQLCursor, u32 ) {

    info!("View Resolver received view_obj: {:?}", view_obj);

    let mut filter_data = view_obj.filter_data.clone();

    let mut view_cursor =  if let Some(cursor) = cursor_option { cursor } else { GraphQLCursor::default() };

    debug!("View Cursor: {:?}", view_cursor);


    let mut request_num: u32 = 0;
    let found_issue_list: Vec<Value> = single_endpoint_fetch(
        &mut view_cursor, &mut request_num,
        &mut filter_data, &linear_config).await;


    info!("'optimized_view_issue_fetch' returning found_issue_list.len(): {:?}", found_issue_list.len());
    info!("'optimized_view_issue_fetch' returning found_issue_list: {:?}", found_issue_list);

    (found_issue_list, view_cursor, request_num)
}