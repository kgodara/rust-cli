

use std::fmt::Write;

use crate::app;
use crate::util;

use app::App as App;

use crate::components::{ 
    dashboard_view_display::DashboardViewDisplay,
    dashboard_view_panel::DashboardViewPanel,
    linear_custom_view_select::LinearCustomViewSelect,
    linear_workflow_state_display::LinearWorkflowStateDisplayState,
};

use crate::util::{
    colors,
    ui,
    ui::{ TableStyle, hex_str_from_style_color },
    dashboard::{fetch_selected_view_panel_issue, fetch_selected_view_panel_idx},    
    layout::{ widths_from_rect },
};

use crate::constants::table_columns::{ DASHBOARD_VIEW_CONFIG_COLUMNS, CUSTOM_VIEW_SELECT_COLUMNS, VIEW_PANEL_COLUMNS };

use tui::{
  backend::Backend,
  layout::{Alignment, Constraint, Direction, Layout, Rect},
  style::{Color, Modifier, Style},
  text::{Span, Spans},
  widgets::{Block, Borders, Clear, List, ListItem, Paragraph, TableState, Wrap},
  Frame,
};

use serde_json::Value;





pub const BASIC_VIEW_HEIGHT: u16 = 6;
pub const SMALL_TERMINAL_WIDTH: u16 = 150;
pub const SMALL_TERMINAL_HEIGHT: u16 = 45;



pub fn draw_action_select<B>(f: &mut Frame<B>, app: & mut App)
where
  B: Backend,
{
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(20), Constraint::Percentage(70), Constraint::Percentage(10)].as_ref())
        .split(f.size());

    // Render the View Panel Command Bar

    // Determine which Commands are allowed based on state of selection
    let mut modify_workflow_state_cmd_active = false;
    let mut refresh_cmd_active = false;

    // If a View Panel is selected & it is not loading, allow Refresh command
    if let Some(selected_view_panel_idx) = fetch_selected_view_panel_idx(app) {
        // Fetch selected ViewPanel
        let view_panel_list_lock = app.linear_dashboard_view_panel_list.lock().unwrap();

        if let Some(x) = view_panel_list_lock.get(selected_view_panel_idx-1) {
            let view_panel_loading_lock = x.loading.lock().unwrap();
            
            refresh_cmd_active = !*view_panel_loading_lock;   
            drop(view_panel_loading_lock);
        }
        drop(view_panel_list_lock);
    }

    // If a View Panel Issue is selected, allow ModifyWorkflowState command
    if fetch_selected_view_panel_issue(app).is_some() {
        modify_workflow_state_cmd_active = true;
    }

    // Update Command statuses
    app.view_panel_cmd_bar.set_modify_workflow_state_active(modify_workflow_state_cmd_active);
    app.view_panel_cmd_bar.set_refresh_panel_active(refresh_cmd_active);

    // Render command bar
    if let Ok(cmd_items) = app.view_panel_cmd_bar.render() {
        f.render_widget(cmd_items, chunks[0]);
    } else {
        error!("draw_action_select - app.view_panel_cmd_bar.render() failed");
        panic!("draw_action_select - app.view_panel_cmd_bar.render() failed");
    }

    
    // Iterate through the list of View Panels & render each to the appropriate position within layour

    let view_panel_handle = app.linear_dashboard_view_panel_list.lock().unwrap();
    let num_views = view_panel_handle.len();

    for (i, e) in view_panel_handle.iter().enumerate() {
        let view_data_handle = e.issue_table_data.lock().unwrap();
        let selected_view_idx = if let Some(selected_idx) = app.linear_dashboard_view_panel_selected { Some(selected_idx as u16)}
                                    else {None};

        // Clone request_num to a u32
        let req_num_handle = e.request_num.lock().unwrap();
        let req_num: u32 = *req_num_handle;
        drop(req_num_handle);

        // Get bounding-box for view panel
        let view_panel_rect = match num_views {
            1 => { ui::single_view_layout(i, chunks[1]) },
            2 => { ui::double_view_layout(i, chunks[1]) },
            3 => { ui::three_view_layout(i, chunks[1]) },
            4 => { ui::four_view_layout(i, chunks[1]) },
            5 => { ui::five_view_layout(i, chunks[1]) },
            6 => { ui::six_view_layout(i, chunks[1]) },
            _ => {continue;},
        };

        // subtract 2 from width to account for single character table borders
        let view_panel_content_rect = Rect::new(view_panel_rect.x, view_panel_rect.y, view_panel_rect.width-2, view_panel_rect.height);

        let widths: Vec<Constraint> = widths_from_rect( &view_panel_content_rect, &*VIEW_PANEL_COLUMNS);


        // Create TableStyle for ViewPanel

        let highlight_table: bool = 
            if let Some(selected_idx) = selected_view_idx {
                selected_idx == ((i as u16)+1)
            } else {
                false
            };
        
        // Get 'loading' bool from ViewPanel
        let loading_lock = e.loading.lock().unwrap();
        let loading_state: bool = *loading_lock;
        drop(loading_lock);


        let table_style = TableStyle { title_style: Some(( e.filter["name"].clone(), e.filter["color"].clone() )),
            row_bottom_margin: Some(0),
            view_idx: Some((i as u16)+1),
            highlight_table,
            req_num: Some(req_num as u16),
            loading: loading_state,
            loader_state: app.loader_tick
        };


        if let Ok(mut view_panel_table) =
            DashboardViewPanel::render(&view_data_handle,
                &widths,
                table_style
            )
        {

            // Determine if this view panel is currently selected
            let mut is_selected = false;
            if let Some(selected_view_panel_idx) = app.linear_dashboard_view_panel_selected {
                if selected_view_panel_idx == (i+1) {
                    is_selected = true;
                }
            }

            // Determine the correct TableState, depending on if this view is selected or not
            let table_state_option = if is_selected { app.view_panel_issue_selected.clone() } else { None };

            let mut table_state = match table_state_option {
                Some(table_state_val) => { table_state_val },
                None => { TableState::default() }
            };

            view_panel_table = view_panel_table.widths(&widths);

            f.render_stateful_widget(view_panel_table, view_panel_rect, &mut table_state);
        }
    }

    drop(view_panel_handle);


    // Render the action list
    let items: Vec<ListItem> = app
      .actions
      .items
      .iter()
      .map(|i| {
          let mut lines = vec![Spans::from(*i)];
          /*
          for _ in 0..i.1 {
              lines.push(Spans::from(Span::styled(
                  "Lorem ipsum dolor sit amet, consectetur adipiscing elit.",
                  Style::default().add_modifier(Modifier::ITALIC),
              )));
          }
          */
          ListItem::new(lines).style(Style::default().fg(Color::Black).bg(Color::White))
      })
      .collect();

    // Create a List from all list items and highlight the currently selected one
    let items = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Action Select"))
        .highlight_style(
            Style::default()
                .bg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");

    f.render_stateful_widget(items, chunks[2], &mut app.actions.state);

    // Draw Workflow State Selection 
    if app.linear_draw_workflow_state_select {
        // debug!("draw_action_select - app.linear_draw_workflow_state_select is true");

        let table;
        let table_result;
        let workflow_states_handle = app.linear_workflow_select.workflow_states_data.lock().unwrap();

        let cloned_states_vec: Vec<Value> = workflow_states_handle.clone();
        table_result = LinearWorkflowStateDisplayState::get_rendered_workflow_state_select(&cloned_states_vec).to_owned();
        drop(workflow_states_handle);

        match table_result {
            Ok(x) => { table = x },
            Err(_) => {return;},
        }

        // debug!("draw_action_select - get_rendered_workflow_state_select() success");

        let area = util::ui::centered_rect(40, 80, f.size());

        let workflow_chunks = Layout::default()
                                .direction(Direction::Vertical)
                                .constraints([Constraint::Percentage(30), Constraint::Percentage(70)].as_ref())
                                .split(area);


        f.render_widget(Clear, area); //this clears out the background

        // debug!("draw_action_select - about to call fetch_selected_view_panel_issue()");

        let selected_issue_opt = fetch_selected_view_panel_issue(&app);
        let selected_issue;

        // debug!("draw_action_select - fetch_selected_view_panel_issue() success");


        // Check that an Issue is selected, if not:
        // don't render workflow pop-up and set linear_draw_workflow_state_select = false
        if let Some(x) = selected_issue_opt {
            selected_issue = x;
        }
        else {
            error!("Workflow States Component cannot render without an Issue selected");
            app.linear_draw_workflow_state_select = false;
            return;
        }
        // Get 'color', 'number', and 'title' field from selected Issue

        let selected_issue_color: Value = selected_issue["state"]["color"].clone();
        let selected_issue_number: Value = selected_issue["number"].clone();
        let selected_issue_title: Value = selected_issue["title"].clone();

        // Return if one of the issue's fields was not found
        if  selected_issue_color.is_null() ||
            selected_issue_number.is_null() ||
            selected_issue_title.is_null()
        {
            error!("Workflow States Component failed to acquire all required Issue fields - color: {:?}, number: {:?}, title: {:?}", selected_issue_color, selected_issue_number, selected_issue_title);
            app.linear_draw_workflow_state_select = false;
            return;
        }

        // debug!("draw_action_select - Get 'color', 'number', and 'title' field from selected Issue success");



        let mut final_title = String::new();
        
        write!( &mut final_title,
                "{} - {}",
                selected_issue_number.as_i64().get_or_insert(-0),
                selected_issue_title.as_str().get_or_insert("ERR TITLE NOT FOUND")
        );

        // info!("Workflow State Selection final Issue Title: {}", final_title);

        let mut title_color = util::ui::style_color_from_hex_str(&selected_issue_color);

        // Render Issue title in upper chunk
        let text = vec![
            Spans::from(Span::styled(
                final_title,
                Style::default().fg(*title_color.get_or_insert(Color::Green)).add_modifier(Modifier::ITALIC),
            ))
        ];

        let paragraph = Paragraph::new(text.clone())
            .block(Block::default()/*.title("Left Block")*/.borders(Borders::ALL))
            .alignment(Alignment::Left).wrap(Wrap { trim: true });
        
        // debug!("draw_action_select - rendering workflow state components");
        
        f.render_widget(paragraph, workflow_chunks[0]);

        let mut table_state = app.linear_workflow_select.workflow_states_state.clone();

        // Render workflow state selection table in lower chunk
        f.render_stateful_widget(table, workflow_chunks[1], &mut table_state);
    }
}

pub fn draw_dashboard_view_display<B>(f: &mut Frame<B>, app: &mut App)
where
  B: Backend,
{


    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(20), Constraint::Percentage(80)].as_ref())
        .split(f.size());

    // Draw Command Bar to display Applicable Commands

    // Get Selected Custom View from app.linear_dashboard_view_list using app.linear_dashboard_view_idx
    let mut view_is_selected = false;
    let mut selected_view: Option<Value> = None;

    if let Some(view_idx) = app.linear_dashboard_view_idx {
        view_is_selected = true;
        selected_view = app.linear_dashboard_view_list[view_idx].clone();
    }

    // Determine which Commands are allowed based on state of selection
    let mut remove_view_cmd_active = false;

    // If a View is not selected, no Commands allowed
    if view_is_selected {
        // A filled view slot is selected, allow Replace View and Remove View Commands
        if selected_view.is_some() {
            remove_view_cmd_active = true;
        }
    }

    // Update Command statuses
    debug!("remove_view_cmd_active: {:?}", remove_view_cmd_active);
    app.dashboard_view_config_cmd_bar.set_remove_view_active(remove_view_cmd_active);

    // Render command bar
    if let Ok(cmd_items) = app.dashboard_view_config_cmd_bar.render() {
        f.render_widget(cmd_items, chunks[0]);
    } else {
        error!("draw_dashboard_view_display - app.dashboard_view_config_cmd_bar.render() failed");
        panic!("draw_dashboard_view_display - app.dashboard_view_config_cmd_bar.render() failed");
    }


    // Get Rects for DashboardViewDisplay & CustomViewSelect
    let bottom_row_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage(50),
                Constraint::Percentage(50),
            ]
            .as_ref(),
        )
        .split(chunks[1]);



    // Draw Dashboard View Display

    // Create TableStyle for Dashboard View List
    let view_list_table_style = TableStyle { 
        title_style: 
        Some((
            Value::String(String::from("Dashboard View Configuration")),
            Value::String( hex_str_from_style_color(&colors::DASHBOARD_VIEW_LIST_TABLE_TITLE).unwrap_or_else(|| String::from("#000000")) ) )),
        row_bottom_margin: Some(0),
        view_idx: Some(1),
        highlight_table: app.linear_dashboard_view_list_selected,
        req_num: None,
        loading: false,
        loader_state: app.loader_tick,
    };

    // subtract 2 from width to account for single character table borders
    let view_display_content_rect = Rect::new(bottom_row_chunks[0].x, bottom_row_chunks[0].y, bottom_row_chunks[0].width-2, bottom_row_chunks[0].height);

    // let widths: Vec<Constraint> = widths_from_rect( &bottom_row_chunks[0], &*DASHBOARD_VIEW_CONFIG_COLUMNS);
    let widths: Vec<Constraint> = widths_from_rect( &view_display_content_rect, &*DASHBOARD_VIEW_CONFIG_COLUMNS);

    if let Ok(mut view_display_table) = 
        DashboardViewDisplay::render(&app.linear_dashboard_view_list, &widths, view_list_table_style)
    {

        view_display_table = view_display_table.widths(&widths);


        let mut table_state = app.dashboard_view_display.view_table_state.clone();


        f.render_stateful_widget(view_display_table, bottom_row_chunks[0], &mut table_state);
    } else {
        error!("draw_dashboard_view_display - DashboardViewDisplay::get_rendered_view_table failed");
        panic!("draw_dashboard_view_display - DashboardViewDisplay::get_rendered_view_table failed");
    }


    // Draw Custom View Select
  
    let view_data_handle = &app.linear_custom_view_select.view_table_data.lock().unwrap();

    let custom_view_select_loading_lock = app.linear_custom_view_select.loading.lock().unwrap();

    // Create TableStyle for Custom View Select
    let custom_view_select_table_style = TableStyle { 
        title_style: 
        Some((
            Value::String(String::from("Custom View Select")), 
            Value::String( hex_str_from_style_color(&colors::CUSTOM_VIEW_SELECT_TABLE_TITLE).unwrap_or_else(|| String::from("#000000")) ) )),
        row_bottom_margin: Some(0),
        view_idx: Some(2),
        highlight_table: !app.linear_dashboard_view_list_selected,
        req_num: None,
        loading: *custom_view_select_loading_lock,
        loader_state: app.loader_tick,
    };

    drop(custom_view_select_loading_lock);

    // subtract 2 from width to account for single character table borders
    let view_select_content_rect = Rect::new(bottom_row_chunks[1].x, bottom_row_chunks[1].y, bottom_row_chunks[1].width-2, bottom_row_chunks[1].height);

    // lazy_static! provides a struct which dereferences towards target struct, hence: '&*'
    // https://github.com/rust-lang-nursery/lazy-static.rs/issues/119#issuecomment-419595818
    let widths: Vec<Constraint> = widths_from_rect( &view_select_content_rect, &*CUSTOM_VIEW_SELECT_COLUMNS);

    if let Ok(mut view_select_table) = LinearCustomViewSelect::render(view_data_handle,
        &widths,
        custom_view_select_table_style)
    {
        view_select_table = view_select_table.widths(&widths);

        let mut custom_view_table_state = app.linear_custom_view_select.view_table_state.clone();

        f.render_stateful_widget(view_select_table, bottom_row_chunks[1], &mut custom_view_table_state);
    } else {
        error!("draw_dashboard_view_display - LinearCustomViewSelect::get_rendered_view_data failed");
        panic!("draw_dashboard_view_display - LinearCustomViewSelect::get_rendered_view_data failed");
    }

  /*
    if None == app.linear_custom_view_select.view_table_state.selected() {
        let custom_view_select_handle = app.linear_custom_view_select.view_table_data.lock().unwrap();

        if let Some(custom_view_data) = &*custom_view_select_handle {
        if let Value::Array(custom_view_vec) = custom_view_data {
            if custom_view_vec.len() > 0 {
                let mut table_state = TableState::default();
                state_table::next(&mut table_state, &custom_view_vec);
                app.linear_custom_view_select.view_table_state = table_state.clone();
            }
        }
        }
    }
  */

}