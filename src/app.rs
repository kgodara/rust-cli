use crate::util;
use crate::linear;
use crate::network;

use network::IOEvent as IOEvent;


use tokio::{
    time::{ sleep, Duration },
    sync::oneshot
};

use std::sync::{
    Arc,
    Mutex,
    atomic::{
        AtomicBool,
        Ordering
    }
};

use crate::constants::{
    IssueModificationOp
};

use crate::linear::{
    LinearConfig,
    view_resolver::ViewLoader
};

use serde_json::Value;

use std::collections::{HashSet, HashMap};

use crate::util::{
    StatefulList as StatefulList,
    GraphQLCursor,
    dashboard::fetch_selected_view_panel_issue,
    dashboard::fetch_selected_value,
};

use crate::components::{
    command_bar::{ CommandBar, CommandBarType },

    user_input::{ UserInput, TokenValidationState },

    linear_custom_view_select::LinearCustomViewSelect,

    dashboard_view_display::DashboardViewDisplay,
    dashboard_view_panel::DashboardViewPanel,

    linear_issue_op_interface::LinearIssueOpInterface,
};

use tui::{
    widgets::{ TableState },
};

pub struct ViewLoadBundle {
    pub linear_config: LinearConfig,

    pub tz_name_offset_lookup: Arc<Mutex<HashMap<String, f64>>>,

    pub item_filter: Value,
    pub table_data: Arc<Mutex<Vec<Value>>>,
    pub loader: Arc<Mutex<Option<ViewLoader>>>,
    pub request_num: Arc<Mutex<u32>>,
    pub loading: Arc<AtomicBool>,

    pub tx: tokio::sync::mpsc::Sender<IOEvent>,
}

#[derive(PartialEq)]
pub enum Route {
    ConfigInterface,
    ActionSelect,
    DashboardViewDisplay
}

#[derive(PartialEq)]
pub enum InputMode {
    Normal,
    Editing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Platform {
    Na,
    Linear,
    Github,
}
// linear_team_select

// App holds the state of the application
pub struct App<'a> {
    // current route
    pub route: Route,
    /// Current value of the Command string
    pub cmd_str: String,
    // LinearClient
    pub linear_client: linear::client::LinearClient,

    // Config Interface
    pub config_interface_input: UserInput,
    // Current input mode
    pub input_mode: InputMode,
    // Current submitted access token to validate
    // pub access_token_to_validate: String,

    // loader_tick is a looping index for loader_state
    pub loader_tick: u16,

    // scroll_tick is an index which loops over 100 for paragraph scrolling 
    pub scroll_tick: u64,

    // has previously cached view list been checked for
    pub view_list_cache_read_attempted: bool,

    // TimeZone Manager
    pub tz_name_offset_map: Arc<Mutex<HashMap<String, f64>>>,

    pub team_tz_map: Arc<Mutex<HashMap<String, String>>>,
    pub team_tz_load_in_progress: Arc<AtomicBool>,
    pub team_tz_load_done: Arc<AtomicBool>,

    // Linear Custom View Select
    pub linear_custom_view_select: LinearCustomViewSelect,
    // Selected Custom View
    pub linear_selected_custom_view_idx: Option<usize>,
    // Linear Custom View Cursor
    pub linear_custom_view_cursor: Arc<Mutex<GraphQLCursor>>,

    // Linear Dashboard Custom View List Display
    pub dashboard_view_display: DashboardViewDisplay,
    pub dashboard_view_config_cmd_bar: CommandBar<'a>,

    // Linear Dashboard Custom View List
    pub linear_dashboard_view_list: Vec<Option<Value>>,
    pub linear_dashboard_view_idx: Option<usize>,
    pub linear_dashboard_view_list_selected: bool,

    // Linear Dashboard View Panel Display

    // Linear Dashboard 'DashboardViewPanel' components
    pub linear_dashboard_view_panel_list: Arc<Mutex<Vec<DashboardViewPanel>>>,
    pub linear_dashboard_view_panel_selected: Option<usize>,
    pub view_panel_issue_selected: Option<TableState>,
    pub view_panel_to_paginate: usize,

    pub view_panel_cmd_bar: CommandBar<'a>,

    pub issue_to_expand: Option<Value>,

    // Issue Modification fields
    pub modifying_issue: bool,
    pub linear_issue_op_interface: LinearIssueOpInterface,

    // Available actions
    pub actions: StatefulList<&'a str>,
}



impl<'a> Default for App<'a> {
    fn default() -> App<'a> {
        App {
            route: Route::ConfigInterface,
            cmd_str: String::new(),

            linear_client: linear::client::LinearClient::default(),

            config_interface_input: UserInput::default(),
            input_mode: InputMode::Normal,
            // access_token_to_validate: String::from(""),

            loader_tick: 0,
            scroll_tick: 0,

            view_list_cache_read_attempted: false,

            tz_name_offset_map: Arc::new(Mutex::new(linear::parse_timezones_from_file())),

            team_tz_map: Arc::new(Mutex::new(HashMap::new())),
            team_tz_load_in_progress: Arc::new(AtomicBool::new(false)),
            team_tz_load_done: Arc::new(AtomicBool::new(false)),

            linear_custom_view_select: LinearCustomViewSelect::default(),
            linear_selected_custom_view_idx: None,
            linear_custom_view_cursor: Arc::new(Mutex::new(GraphQLCursor::default())),

            dashboard_view_display: DashboardViewDisplay::default(),
            dashboard_view_config_cmd_bar: CommandBar::with_type(CommandBarType::ViewList),


            linear_dashboard_view_list: vec![ None, None, None, None, None, None ],
            linear_dashboard_view_idx: None,
            linear_dashboard_view_list_selected: true,

            linear_dashboard_view_panel_list: Arc::new(Mutex::new(Vec::with_capacity(6))),
            linear_dashboard_view_panel_selected: None,
            view_panel_issue_selected: None,
            view_panel_to_paginate: 0,

            view_panel_cmd_bar: CommandBar::with_type(CommandBarType::Dashboard),

            issue_to_expand: None,

            modifying_issue: false,
            linear_issue_op_interface: LinearIssueOpInterface::default(),

            actions: util::StatefulList::with_items(vec![
                "Modify Dashboard",
            ]).selected(),
        }
    }
}







impl<'a> App<'a> {


    pub fn change_route(&mut self, route: Route, tx: &tokio::sync::mpsc::Sender<IOEvent>) {
        match route {

            Route::ConfigInterface => {
                // Unselect from actions list
                self.actions.unselect();
            },

            // Create DashboardViewPanel components for each Some in app.linear_dashboard_view_list
            // and set app.linear_dashboard_view_panel_list
            // Load all Dashboard Views
            Route::ActionSelect => {
                // Select first action
                self.actions.next();

                if !self.view_list_cache_read_attempted {
                    let cached_read_option = LinearConfig::read_view_list();
                    if let Some(cached_view_list) = cached_read_option {
                        self.linear_dashboard_view_list = cached_view_list;
                    }
                }

                self.dispatch_event("load_dashboard_views", &tx);
            },

            Route::DashboardViewDisplay => {

                /*
                // TODO: Clear any previous CustomViewSelect related values on self
                self.linear_custom_view_select = components::linear_custom_view_select::LinearCustomViewSelect::default();
                self.linear_selected_custom_view_idx = None;
                self.linear_custom_view_cursor = Arc::new(Mutex::new(GraphQLCursor::default()));

                self.dispatch_event("load_custom_views", tx);
                */

                // Unselect from actions list
                self.actions.unselect();

                // TODO: Clear any previous CustomViewSelect related values on self
                self.linear_custom_view_select = LinearCustomViewSelect::default();
                self.linear_selected_custom_view_idx = None;
                self.linear_custom_view_cursor = Arc::new(Mutex::new(GraphQLCursor::default()));

                self.linear_dashboard_view_list_selected = true;

                self.dispatch_event("load_custom_views", tx);
            }
        }
        self.route = route;
    }

    pub fn dispatch_event(&mut self, event_name: &str, tx: &tokio::sync::mpsc::Sender<IOEvent>) {

        match event_name {

            "load_viewer" => {

                let tx2 = tx.clone();

                let token_validation_state_handle = self.config_interface_input.token_validation_state.clone();
                {
                    let mut token_validation_state_lock = token_validation_state_handle.lock().unwrap();
                    *token_validation_state_lock = TokenValidationState::Validating;
                }
                

                let token: String = self.config_interface_input.input.clone();

                let linear_config_handle = self.linear_client.config.clone();

                let _t1 = tokio::spawn(async move {

                    let (resp_tx, resp_rx) = oneshot::channel();

                    let cmd = IOEvent::LoadViewer { api_key: token.clone(),
                                                            resp: resp_tx };
                    tx2.send(cmd).await.unwrap();

                    let res = resp_rx.await.ok();

                    info!("LoadViewer IOEvent returned: {:?}", res);

                    let mut token_validation_state_lock = token_validation_state_handle.lock().unwrap();

                    // Check for "errors" field, if not found save access token
                    if let Some(Some(resp_json)) = res {
                        let req_failed: bool = match resp_json["error_node"] {
                            Value::Null => {false},
                            _ => {true}
                        };

                        if !req_failed {
                            if let Value::Object(viewer) = &resp_json["viewer_node"] {
                                // save entered token to file
                                {
                                    let mut linear_config_lock = linear_config_handle.lock().unwrap();
                                    linear_config_lock.save_access_token(&token);
                                    linear_config_lock.save_viewer_object(viewer.clone());

                                    *token_validation_state_lock = TokenValidationState::Valid;
                                }
                            }
                        } else {
                            *token_validation_state_lock = TokenValidationState::Invalid;
                        }
                    } else {
                        *token_validation_state_lock = TokenValidationState::Invalid;
                    }

                });
            },

            "load_custom_views" => {
                // TODO: Clear any previous CustomViewSelect related values on self


                let view_select_loading_handle = self.linear_custom_view_select.loading.clone();
                // If already loading something, don't try again
                if view_select_loading_handle.load(Ordering::Relaxed) {
                    return;
                }
                // Set Loading 'true' before fetch
                view_select_loading_handle.store(true, Ordering::Relaxed);


                let tx2 = tx.clone();

                let linear_config_lock = self.linear_client.config.lock().unwrap();
                let linear_config = linear_config_lock.clone();
                drop(linear_config_lock);

                let view_data_handle = self.linear_custom_view_select.view_table_data.clone();


                let view_cursor_handle = self.linear_custom_view_cursor.lock().unwrap();
                let view_cursor: GraphQLCursor = view_cursor_handle.clone();
                drop(view_cursor_handle);

                let view_cursor_handle = self.linear_custom_view_cursor.clone();

                let _t1 = tokio::spawn(async move {

                    let (resp_tx, resp_rx) = oneshot::channel();

                    let cmd = IOEvent::LoadCustomViews { linear_config,
                                                            linear_cursor: view_cursor,
                                                            resp: resp_tx };
                    tx2.send(cmd).await.unwrap();

                    let res = resp_rx.await.ok();

                    info!("LoadCustomViews IOEvent returned: {:?}", res);

                    let mut view_data_lock = view_data_handle.lock().unwrap();
                    let mut view_cursor_data_lock = view_cursor_handle.lock().unwrap();

                    let mut current_views = view_data_lock.clone();

                    if let Some(Some(mut y)) = res {

                        if let Some(new_views_vec) = y["views"].as_array_mut() {
                            current_views.append(new_views_vec);
                            *view_data_lock = current_views;
                            view_select_loading_handle.store(false, Ordering::Relaxed);
                        }

                        match GraphQLCursor::linear_cursor_from_page_info(y["cursor_info"].clone()) {
                            Some(z) => {
                                info!("Updating view_cursor_data_lock to: {:?}", z);
                                *view_cursor_data_lock = z;
                            },
                            None => {
                                error!("'load_custom_views' linear_cursor_from_page_info() failed for cursor_info: {:?}", y["cursor_info"]);
                                panic!("'load_custom_views' linear_cursor_from_page_info() failed for cursor_info: {:?}", y["cursor_info"]);
                            },
                        }
                    }

                    info!("New self.linear_custom_view_select.view_table_data: {:?}", view_data_lock);
                });
            },

            "load_dashboard_views" => {
                // Reset app.linear_dashboard_view_panel_list
                let view_panel_list_ref = self.linear_dashboard_view_panel_list.clone();
                let mut view_panel_list_handle = view_panel_list_ref.lock().unwrap();

                // view_panel_list_handle.clear();

                let mut existing_panel_set = HashSet::new();

                debug!("dispatch_event::load_dashboard_views - self.linear_dashboard_view_list: {:?}", self.linear_dashboard_view_list);

                for (i, filter_opt) in self.linear_dashboard_view_list.iter().enumerate() {
                    //  If a View Panel for the filter is present within self.linear_dashboard_view_panel_list
                    //  and self.linear_dashboard_view_panel_list[x].is_loading == false,
                    //      if the index doesn't match:
                    //          clone the view panel and insert into the correct index within self.linear_dashboard_view_panel_list
                    //      else:
                    //          do not insert a new view panel

                    if let Some(filter) = filter_opt {
                        // Create DashboardViewPanels for each filter

                        let filter_id = filter["id"].clone();
                        let filter_view_panel_exists = view_panel_list_handle
                                                        .iter()
                                                        .position(|e| { 
                                                            debug!("filter_view_panel_exists comparing {:?} == {:?}", e.filter["id"], filter_id);   
                                                            e.filter["id"] == filter_id
                                                        });
                        debug!("i: {:?}, filter_view_panel_exists: {:?}", i, filter_view_panel_exists);


                        match filter_view_panel_exists {
                            Some(filter_view_panel_idx) => {

                                //  if the index doesn't match:
                                //      clone the view panel and replace into the correct index
                                //      within self.linear_dashboard_view_panel_list

                                if i != filter_view_panel_idx {
                                    let dup_view_panel = view_panel_list_handle[filter_view_panel_idx].clone();
                                    // view_panel_list_handle.insert(i, dup_view_panel);
                                    if i < view_panel_list_handle.len() {
                                        let _got = std::mem::replace(&mut view_panel_list_handle[i], dup_view_panel);
                                    }
                                    else {
                                        view_panel_list_handle.insert(i, dup_view_panel);
                                    }
                                }

                                // TODO: Why is this not in an else?
                                // if the index does match, then a ViewPanel already exists for this filter, skip
                                existing_panel_set.insert(i);

                            },
                            // Need to create a new View Panel
                            None => {
                                debug!("Attempting to use insert for i: {:?}", i);
                                // view_panel_list_handle.insert(i, DashboardViewPanel::with_filter(filter.clone()));
                                // let got = std::mem::replace(&mut view_panel_list_handle[i], DashboardViewPanel::with_filter(filter.clone()));

                                if i < view_panel_list_handle.len() {
                                    let _got = std::mem::replace(&mut view_panel_list_handle[i], DashboardViewPanel::with_filter(filter.clone()));
                                }
                                else {
                                    view_panel_list_handle.insert(i, DashboardViewPanel::with_filter(filter.clone()));
                                }
                            }
                        };
                    }
                }

                info!("change_route ActionSelect new self.linear_dashboard_view_panel_list: {:?}", view_panel_list_handle);
                
                let linear_config_lock = self.linear_client.config.lock().unwrap();
                let linear_config = linear_config_lock.clone();
                drop(linear_config_lock);

                // Create 'view_load_bundles': Vec<ViewLoadBundle> from view_panel_list_handle
                // Filter to only create ViewLoadBundles for ViewPanels where 
                let view_load_bundles: Vec<ViewLoadBundle> = view_panel_list_handle
                    .iter()
                    .cloned()
                    .enumerate()
                    .filter_map(|(i, e)| {
                        if existing_panel_set.contains(&i) {
                            None
                        }
                        else {
                            Some(ViewLoadBundle {
                                            linear_config: linear_config.clone(),

                                            tz_name_offset_lookup: self.tz_name_offset_map.clone(),
                                            
                                            item_filter: e.filter,
                                            table_data: e.issue_table_data.clone(),
                                            loader: e.view_loader.clone(),
                                            request_num: e.request_num.clone(),
                                            loading: e.loading.clone(),

                                            tx: tx.clone(),
                                        })
                        }
                    })
                    .collect();



                drop(view_panel_list_handle);

                // timezone load completion bool handle
                let team_tz_load_done_handle = self.team_tz_load_done.clone();
                let team_tz_lookup_handle = self.team_tz_map.clone();


                let _t1 = tokio::spawn(async move {

                    // Load all DashboardViewPanels
                    
                    // Loop here and wait for timezone load to complete
                    loop {
                        sleep(Duration::from_millis(10)).await;
                        {
                            if team_tz_load_done_handle.load(Ordering::Relaxed) {
                                break;
                            }
                        }
                    }

                    // Fetch self.team_tz_map here
                    let team_tz_lookup_parent: HashMap<String, String>;
                    {
                        let team_tz_lookup_lock = team_tz_lookup_handle.lock().unwrap();
                        team_tz_lookup_parent = team_tz_lookup_lock.clone();
                    }
                    

                    // note the use of `into_iter()` to consume `items`
                    let tasks: Vec<_> = view_load_bundles
                        .into_iter()
                        .map(|item| {
                            // item is: 
                            /*
                            pub struct DashboardViewPanel {
                                pub filter: Value,
                                pub issue_table_data: Arc<Mutex<Option<Value>>>,
                            }
                            */
                            info!("Spawning Get View Panel Issues Task");

                            let loader_handle = item.loader.lock().unwrap();
                            let loader = loader_handle.clone();
                            drop(loader_handle);

                            let team_tz_lookup = team_tz_lookup_parent.clone();

                            // Set ViewPanel loading state to true
                            item.loading.store(true, Ordering::Relaxed);

                            tokio::spawn(async move {
                                let (resp_tx, resp_rx) = oneshot::channel();


                                let cmd = IOEvent::LoadViewIssues { linear_config: item.linear_config.clone(),
                                                                    team_tz_lookup: team_tz_lookup.clone(),
                                                                    tz_offset_lookup: item.tz_name_offset_lookup,
                                                                    issue_data: Arc::new(Mutex::new(Vec::new())),
                                                                    view: item.item_filter.clone(), 
                                                                    view_loader: loader,
                                                                    resp: resp_tx };
 
                                item.tx.send(cmd).await.unwrap();
            
                                let res = resp_rx.await.ok();

                                info!("LoadViewIssues IOEvent returned: {:?}", res);

                                let mut view_panel_data_lock = item.table_data.lock().unwrap();
                                let mut loader_handle = item.loader.lock().unwrap();
                                let mut request_num_lock = item.request_num.lock().unwrap();

                                if let Some(x) = res {
                                    *view_panel_data_lock = x.0;
                                    *loader_handle = Some(x.1);
                                    *request_num_lock += x.2;
                                    item.loading.store(false, Ordering::Relaxed);
                                }
                                info!("New dashboard_view_panel.issue_table_data: {:?}", view_panel_data_lock);
                            })
                        })
                        .collect();

                    // await the tasks for resolve's to complete and give back our items
                    let mut items = vec![];
                    for task in tasks {
                        items.push(task.await.unwrap());
                    }
                    // verify that we've got the results
                    for item in &items {
                        info!("LoadViewIssues Result: {:?}", item);
                    }
                });

            },
            "paginate_dashboard_view" => {

                let tx2 = tx.clone();

                let view_panel_list_handle = self.linear_dashboard_view_panel_list.lock().unwrap();

                let is_loading = &view_panel_list_handle[self.view_panel_to_paginate].loading;

                // If already loading something, don't try again
                if is_loading.load(Ordering::Relaxed) {
                    return;
                }

                // Set ViewPanel loading state to true
                is_loading.store(true, Ordering::Relaxed);


                let linear_config_lock = self.linear_client.config.lock().unwrap();
                let linear_config = linear_config_lock.clone();
                drop(linear_config_lock);

                let view_panel_view_obj = view_panel_list_handle[self.view_panel_to_paginate].filter.clone();

                let loader_lock = view_panel_list_handle[self.view_panel_to_paginate].view_loader.lock().unwrap();
                let loader = loader_lock.clone();

                let view_panel_issue_handle = view_panel_list_handle[self.view_panel_to_paginate].issue_table_data.clone();
                let loader_handle = view_panel_list_handle[self.view_panel_to_paginate].view_loader.clone();
                let request_num_handle = view_panel_list_handle[self.view_panel_to_paginate].request_num.clone();


                let loading_handle = view_panel_list_handle[self.view_panel_to_paginate].loading.clone();

                let tz_id_name_lookup_dup = self.team_tz_map.lock()
                                                            .unwrap()
                                                            .clone();
                let tz_name_offset_lookup_dup = self.tz_name_offset_map.clone();


                drop(loader_lock);
                drop(view_panel_list_handle);


                let _t1 = tokio::spawn(async move {
                    let (resp_tx, resp_rx) = oneshot::channel();


                    let cmd = IOEvent::LoadViewIssues { linear_config,
                                                        team_tz_lookup: tz_id_name_lookup_dup,
                                                        tz_offset_lookup: tz_name_offset_lookup_dup,
                                                        issue_data: view_panel_issue_handle.clone(),
                                                        view: view_panel_view_obj, 
                                                        view_loader: loader,
                                                        resp: resp_tx };
                    
                    tx2.send(cmd).await.unwrap();

                    let res = resp_rx.await.ok();

                    info!("LoadViewIssues IOEvent returned: {:?}", res);
                    
                    let mut view_panel_data_lock = view_panel_issue_handle.lock().unwrap();
                    let mut loader = loader_handle.lock().unwrap();
                    let mut request_num_lock = request_num_handle.lock().unwrap();

                    let mut current_view_issues = view_panel_data_lock.clone();

                    if let Some(mut x) = res {

                        current_view_issues.append(&mut x.0);
                        *view_panel_data_lock = current_view_issues.clone();
                        *loader = Some(x.1);
                        *request_num_lock += x.2;
                        loading_handle.store(false, Ordering::Relaxed);

                    }
                    info!("New dashboard_view_panel.issue_table_data: {:?}", view_panel_data_lock);
                });
            },
            "load_issue_op_data" => {
                let tx2 = tx.clone();

                let op_interface_loading_handle = self.linear_issue_op_interface.loading.clone();
                // If already loading something, don't try again
                if op_interface_loading_handle.load(Ordering::Relaxed) {
                    return;
                }
                // Set Loading 'true' before fetch
                op_interface_loading_handle.store(true, Ordering::Relaxed);

                let issue_op_data_handle = self.linear_issue_op_interface.table_data_from_op();
                
                let linear_config_lock = self.linear_client.config.lock().unwrap();
                let linear_config = linear_config_lock.clone();
                drop(linear_config_lock);


                let current_op = self.linear_issue_op_interface.current_op;

                let selected_issue_opt = fetch_selected_view_panel_issue(&self);
                let selected_issue;
                let selected_team;

                // Check that an Issue is selected, if not return
                if let Some(x) = selected_issue_opt {
                    selected_issue = x;
                }
                else {
                    return;
                }

                // Get the Issue's team,
                // panic if not found since every Issue should have a value for ['team']['id']
                selected_team = selected_issue["team"]["id"].clone();

                if selected_team.is_null() {
                    error!("['team']['id'] returned Value::Null for Issue: {:?}", selected_issue);
                    panic!("['team']['id'] returned Value::Null for Issue: {:?}", selected_issue);
                }

                // Get Cursor
                let issue_op_cursor_lock = self.linear_issue_op_interface.cursor.lock().unwrap();
                let issue_op_cursor: GraphQLCursor = issue_op_cursor_lock.clone();
                drop(issue_op_cursor_lock);

                let issue_op_cursor_handle = self.linear_issue_op_interface.cursor.clone();


                let _t1 = tokio::spawn(async move {

                    let (resp_tx, resp_rx) = oneshot::channel();

                    debug!("Dispatching Load-{:?} event", current_op);

                    let cmd = IOEvent::LoadOpData { op: current_op,
                        linear_config,
                        linear_cursor: issue_op_cursor,
                        team: selected_team,
                        resp: resp_tx 
                    };

                    tx2.send(cmd).await.unwrap();

                    let mut res = resp_rx.await.ok();

                    let mut issue_op_cursor_data_lock = issue_op_cursor_handle.lock().unwrap();
                    op_interface_loading_handle.store(false, Ordering::Relaxed);


                    info!("Load-{:?} IOEvent returned: {:?}", current_op, res);

                    let mut issue_op_data_lock = issue_op_data_handle.lock().unwrap();

                    let mut current_issue_op_data = issue_op_data_lock.clone();

                    if let Some(Some(ref mut x)) = res {
                        debug!("x - {:?}", x);
                        if let Some(values_vec) = x["data"].as_array_mut() {
                            current_issue_op_data.append(&mut values_vec.to_vec());
                            *issue_op_data_lock = current_issue_op_data;
                        }

                        match GraphQLCursor::linear_cursor_from_page_info(x["cursor_info"].clone()) {
                            Some(z) => {
                                info!("Updating issue_op_cursor_data_lock to: {:?}", z);
                                *issue_op_cursor_data_lock = z;
                            },
                            None => {
                                error!("'load_issue_op_data' linear_cursor_from_page_info() failed for cursor_info: {:?}", x["cursor_info"]);
                                panic!("'load_issue_op_data' linear_cursor_from_page_info() failed for cursor_info: {:?}", x["cursor_info"]);
                            },
                        }
                    }

                    // info!("New self.linear_workflow_select.workflow_states_data: {:?}", workflow_data_lock);
                });
            }
            "update_issue" => {
                let tx3 = tx.clone();

                let issue_id: String;
                let selected_value_id: String;
                let value_obj;

                // Get relevant issue and selected Value id, return if anything not found
                {
                    let selected_issue_opt = fetch_selected_view_panel_issue(&self);
                    let issue_obj = if let Some(x) = selected_issue_opt { x } else { return; };
                    let issue_id_opt = issue_obj["id"].as_str();

                    let selected_value_opt = fetch_selected_value(&self);
                    value_obj = if let Some(x) = selected_value_opt { x } else { return; };
                    let value_id_opt = value_obj["id"].as_str();

                    if let Some(x) = issue_id_opt {
                        issue_id = String::from(x);
                    }
                    else {
                        return;
                    }

                    if let Some(x) = value_id_opt {
                        selected_value_id = String::from(x);
                    }
                    else {
                        return;
                    }
                }

                debug!("update_issue - issue_id, selected_value_id: {:?}, {:?}", issue_id, selected_value_id);

                let linear_config_lock = self.linear_client.config.lock().unwrap();
                let linear_config = linear_config_lock.clone();
                drop(linear_config_lock);

                let view_panel_list_arc = self.linear_dashboard_view_panel_list.clone();

                let current_op = self.linear_issue_op_interface.current_op;

                // Spawn task to issue command to update workflow state
                let _t3 = tokio::spawn( async move {
                    let (resp2_tx, resp2_rx) = oneshot::channel();

                    let cmd = IOEvent::UpdateIssue {
                        op: current_op,
                        linear_config,
                        issue_id: issue_id.clone(),
                        ref_id: selected_value_id,
                        resp: resp2_tx
                    };

                    tx3.send(cmd).await.unwrap();

                    let res = resp2_rx.await.ok();
                    
                    info!("UpdateIssue IOEvent returned: {:?}", res);

                    // UpdateIssueWorkflowState IOEvent returned: Some(Some(Object({"issue_response": Object({"createdAt": String("2021-02-06T17:47:01.039Z"), "id": String("ace38e69-8a64-46f8-ad57-dc70c61f5599"), "number": Number(11), "title": String("Test Insomnia 1")}), "success": Bool(true)})))
                    // If Some(Some(Object({"success": Bool(true)})))
                    // then can match linear_issue_display.issue_table_data using selected_issue["id"]
                    // and update linear_issue_display.issue_table_data[x]["state"] with selected_workflow_state

                    let mut update_succeeded = false;

                    if let Some(Some(query_response)) = res {
                        if let Value::Bool(value) = query_response["success"] {
                            update_succeeded = value;
                        }
                    }
                    

                    
                    // If update succeeded, iterate over all Issues in all ViewPanels
                    // and set issue["state"] = state_obj 
                    //     where id matches 'issue_id'
                    if update_succeeded {
                        let view_panel_list_handle = view_panel_list_arc.lock().unwrap();
                        for view_panel in view_panel_list_handle.iter() {

                            // Iterate over ViewPanel Issues
                            let mut issue_list_handle = view_panel.issue_table_data.lock().unwrap();

                            for issue_obj in issue_list_handle.iter_mut() {
                                if let Some(panel_issue_id) = issue_obj["id"].as_str() {
                                    if panel_issue_id == issue_id.as_str() {
                                        match current_op {
                                            IssueModificationOp::ModifyWorkflowState => {issue_obj["state"] = value_obj.clone();},
                                            IssueModificationOp::ModifyAssignee => {issue_obj["assignee"] = value_obj.clone();},
                                            IssueModificationOp::ModifyProject => {issue_obj["project"] = value_obj.clone();},
                                            IssueModificationOp::ModifyCycle => {issue_obj["cycle"] = value_obj.clone();},
                                            _ => {}
                                        }
                                    }
                                }
                            }
                        }
                    }
                });
            },

            _ => {},
        }

    }

}