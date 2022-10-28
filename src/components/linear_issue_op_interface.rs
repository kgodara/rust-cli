use std::cmp::max;
use std::sync::{
    Arc,
    Mutex,
    atomic::AtomicBool,
};

use unicode_segmentation::UnicodeSegmentation;

use tui::{
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Row, Table, TableState},
};

use serde_json::{ Value, json, Map };


use crate::linear::{
    client::LinearClient,
    LinearConfig,
    types::{ WorkflowState, User, Project, Cycle, IssueRelatableObject }
};

use crate::util::{
    table::{ empty_str_to_fallback, format_cell_fields,
        get_row_height, row_min_render_height, colored_cell,
        TableStyle, gen_table_title_spans,
    },
    layout::{
        widths_from_rect
    },
    GraphQLCursor
};

use crate::constants::{
    IssueModificationOp,
    colors,
    table_columns::{
        WORKFLOW_STATE_SELECT_COLUMNS, ASSIGNEE_SELECT_COLUMNS,
        PROJECT_SELECT_COLUMNS, CYCLE_SELECT_COLUMNS
    }
};

#[derive(Debug, Default, Clone)]
pub struct ModificationOpData {
    pub workflow_states: Vec<WorkflowState>,
    pub users: Vec<User>,
    pub projects: Vec<Project>,
    pub cycles: Vec<Cycle>,
}

pub struct LinearIssueOpInterface {

    pub current_op: Option<IssueModificationOp>,
    pub selected_idx: Option<usize>,
    pub data_state: TableState,
    pub loading: Arc<AtomicBool>,
    pub cursor: Arc<Mutex<GraphQLCursor>>,

    pub obj_data: Arc<Mutex<ModificationOpData>>,
}


impl LinearIssueOpInterface {

    // loading functions section start

    pub async fn load_op_data(op: &IssueModificationOp,
        linear_config: LinearConfig,
        linear_cursor: Option<GraphQLCursor>,
        team_id: String)-> Option<Value> {

        let mut variables: Map<String, Value> = Map::new();
        variables.insert(String::from("ref"), Value::String(team_id));

        debug!("load_op_data about to dispatch query");

        let data_result = match op {
            IssueModificationOp::WorkflowState => {
                LinearClient::get_workflow_states_by_team(linear_config, linear_cursor, variables).await
            }
            IssueModificationOp::Assignee => {
                LinearClient::get_users_by_team(linear_config, linear_cursor, variables).await
            },
            IssueModificationOp::Project => {
                LinearClient::get_projects_by_team(linear_config, linear_cursor, variables).await
            },
            IssueModificationOp::Cycle => {
                LinearClient::get_cycles_by_team(linear_config, linear_cursor, variables).await
            },
            _ => {
                error!("LinearIssueOpInterface::load_op_data, invalid IssueModificationOp: {:?}", op);
                panic!("LinearIssueOpInterface::load_op_data, invalid IssueModificationOp: {:?}", op);
            }
        };

        let data: Value;
        let cursor_info: Value;

        match data_result {
            Ok(x) => {
                data = x["data_nodes"].clone();
                cursor_info = x["cursor_info"].clone();
            },
            Err(y) => {
                error!("Get data for {:?} failed: {:?}", op, y);
                return None;
            },
        }

        debug!("load_op_data - op, data: {:?}, {:?}", op, data);

        if data == Value::Null {
            return Some(Value::Array(vec![]));
        }

        match data {
            Value::Array(_) => {
                Some(json!( { "data": data, "cursor_info": cursor_info } ))
            },
            _ => {None},
        }
    }

    // loading functions section end

    pub fn table_data_from_op(&self) -> Option<Vec<IssueRelatableObject>> {
        let obj_data_lock = self.obj_data.lock().unwrap();
        match self.current_op {
            Some(IssueModificationOp::WorkflowState) => {
                Some(obj_data_lock.workflow_states
                    .iter()
                    .map(|state| { IssueRelatableObject::WorkflowState(state.clone()) })
                    .collect())
            },
            Some(IssueModificationOp::Assignee) => {
                Some(obj_data_lock.users
                    .iter()
                    .map(|user| { IssueRelatableObject::Assignee(user.clone()) })
                    .collect())
            },
            Some(IssueModificationOp::Project) => {
                Some(obj_data_lock.projects
                    .iter()
                    .map(|project| { IssueRelatableObject::Project(project.clone()) })
                    .collect())
            },
            Some(IssueModificationOp::Cycle) => {
                Some(obj_data_lock.cycles
                    .iter()
                    .map(|cycle| { IssueRelatableObject::Cycle(cycle.clone()) })
                    .collect())
            },
            _ => { None }
        }
    }


    pub fn is_valid_selection_for_update(&self, issue_title_input: &str) -> bool {
        match self.current_op {
            Some(IssueModificationOp::Title) => {
                let grapheme_len: usize = issue_title_input
                    .graphemes(true)
                    .count();
                grapheme_len > 0
            },
            Some(IssueModificationOp::WorkflowState) => {
                self.selected_idx.is_some()
            },
            Some(IssueModificationOp::Assignee) => {
                self.selected_idx.is_some()
            },
            Some(IssueModificationOp::Project) => {
                self.selected_idx.is_some()
            },
            Some(IssueModificationOp::Cycle) => {
                self.selected_idx.is_some()
            },
            _ => {
                false
            }
        }
    }

    pub fn reset_op(&mut self) {
        let mut obj_data_lock = self.obj_data.lock().unwrap();
        match self.current_op {
            Some(IssueModificationOp::Title) => {

            },
            Some(IssueModificationOp::WorkflowState) => {
                obj_data_lock.workflow_states = Vec::default();
            },
            Some(IssueModificationOp::Assignee) => {
                obj_data_lock.users = Vec::default();
            },
            Some(IssueModificationOp::Project) => {
                obj_data_lock.projects = Vec::default();
            },
            Some(IssueModificationOp::Cycle) => {
                obj_data_lock.cycles = Vec::default();
            },
            _ => {
                error!("reset_op: invalid LinearIssueOpInterface::current_op: {:?}", self.current_op);
                panic!("reset_op: invalid LinearIssueOpInterface::current_op: {:?}", self.current_op);
            }
        };

        self.selected_idx = None;
        self.current_op = None;

        self.data_state = TableState::default();
        self.cursor = Arc::new(Mutex::new(GraphQLCursor::default()));
    }

    // render helper functions
    fn cell_fields_from_row(row: &IssueRelatableObject, widths: &[Constraint]) -> Vec<String> {
        let cell_fields: Vec<String>;
        match row {
            IssueRelatableObject::WorkflowState(state) => {
                cell_fields = empty_str_to_fallback(
                    &[
                        &state.name.clone(),
                        &state.state_type.clone(),
                        state.description.as_deref().unwrap_or(""),
                    ],
                    &WORKFLOW_STATE_SELECT_COLUMNS
                );

                let row_height = row_min_render_height(&cell_fields, widths, &WORKFLOW_STATE_SELECT_COLUMNS);

                // Get the formatted Strings for each cell field
                format_cell_fields(&cell_fields, widths, &WORKFLOW_STATE_SELECT_COLUMNS, Some(row_height))
            },
            IssueRelatableObject::Assignee(assignee) => {
                cell_fields = empty_str_to_fallback(
                    &[
                        assignee.name.as_deref().unwrap_or(""),
                        assignee.display_name.as_deref().unwrap_or(""),
                    ],
                    &ASSIGNEE_SELECT_COLUMNS
                );

                let row_height = row_min_render_height(&cell_fields, widths, &ASSIGNEE_SELECT_COLUMNS);

                format_cell_fields(&cell_fields, widths, &ASSIGNEE_SELECT_COLUMNS, Some(row_height))
            },
            IssueRelatableObject::Project(project) => {
                cell_fields = empty_str_to_fallback(
                    &[
                        project.name.as_deref().unwrap_or(""),
                        project.state.as_deref().unwrap_or(""),
                    ],
                    &PROJECT_SELECT_COLUMNS
                );

                let row_height = row_min_render_height(&cell_fields, widths, &PROJECT_SELECT_COLUMNS);

                format_cell_fields(&cell_fields, widths, &PROJECT_SELECT_COLUMNS, Some(row_height))
            },
            IssueRelatableObject::Cycle(cycle) => {
                cell_fields = empty_str_to_fallback(
                    &[
                        cycle.name.as_deref().unwrap_or(""),
                        &cycle.number.to_string().clone(),
                        &cycle.starts_at.clone(),
                        &cycle.ends_at.clone(),
                    ],
                    &CYCLE_SELECT_COLUMNS
                );

                let row_height = row_min_render_height(&cell_fields, widths, &CYCLE_SELECT_COLUMNS);

                format_cell_fields(&cell_fields, widths, &CYCLE_SELECT_COLUMNS, Some(row_height))
            },
        }
    }

    pub fn widths_from_rect_op(bbox: &Rect, op: &IssueModificationOp) -> Vec<Constraint> {
        match op {
            IssueModificationOp::WorkflowState => {
                widths_from_rect(bbox, &WORKFLOW_STATE_SELECT_COLUMNS)
            },
            IssueModificationOp::Assignee => {
                widths_from_rect(bbox, &ASSIGNEE_SELECT_COLUMNS)
            },
            IssueModificationOp::Project => {
                widths_from_rect(bbox, &PROJECT_SELECT_COLUMNS)
            },
            IssueModificationOp::Cycle => {
                widths_from_rect(bbox, &CYCLE_SELECT_COLUMNS)
            },
            _ => {panic!("Not ready")}
        }
    }

    pub fn title_from_op(op: &IssueModificationOp) -> String {
        match op {
            IssueModificationOp::WorkflowState => {
                "Select New Workflow State".to_string()
            },
            IssueModificationOp::Assignee => {
                "Select New Assignee".to_string()
            },
            IssueModificationOp::Project => {
                "Select New Project".to_string()
            },
            IssueModificationOp::Cycle => {
                "Select New Cycle".to_string()
            },
            _ => {
                panic!("Not ready");
            }
        }
    }

    pub fn render<'a>(
        op: IssueModificationOp,
        table_data: &ModificationOpData,
        widths: &[Constraint],
        table_style: TableStyle) -> Result<Table<'a>, &'static str> {

        let bottom_margin = table_style.row_bottom_margin.unwrap_or(0);

        let selected_style = Style::default().add_modifier(Modifier::REVERSED);
        let normal_style = Style::default().bg(Color::DarkGray);


        let header_cells: Vec<Cell> = match op {
                IssueModificationOp::WorkflowState => { &*WORKFLOW_STATE_SELECT_COLUMNS },
                IssueModificationOp::Assignee => { &*ASSIGNEE_SELECT_COLUMNS },
                IssueModificationOp::Project => { &*PROJECT_SELECT_COLUMNS },
                IssueModificationOp::Cycle => { &*CYCLE_SELECT_COLUMNS },
                _ => { 
                    error!("LinearIssueOpInterface::render - header_cells invalid IssueModificationOp: {:?}", op);
                    panic!("LinearIssueOpInterface::render - header_cells invalid IssueModificationOp: {:?}", op);
                }
            }
            .iter()
            .map(|h| Cell::from(&*h.label).style(Style::default().fg(Color::LightGreen)))
            .collect();

        let header = Row::new(header_cells)
            .style(normal_style)
            .height(1)
            .bottom_margin(1);

        let mut max_seen_row_size: usize = 0;

        let obj_vec_to_iter: Vec<IssueRelatableObject> = match op {
            IssueModificationOp::WorkflowState => {
                table_data.workflow_states
                    .iter()
                    .map(|state| { IssueRelatableObject::WorkflowState(state.clone()) })
                    .collect()
            },
            IssueModificationOp::Assignee => {
                table_data.users
                    .iter()
                    .map(|user| { IssueRelatableObject::Assignee(user.clone()) })
                    .collect()
            }
            IssueModificationOp::Project => {
                table_data.projects
                    .iter()
                    .map(|project| { IssueRelatableObject::Project(project.clone()) })
                    .collect()
            }
            IssueModificationOp::Cycle => {
                table_data.cycles
                    .iter()
                    .map(|cycle| { IssueRelatableObject::Cycle(cycle.clone()) })
                    .collect()
            },
            _ => {
                panic!("unsupported op!");
            },
        };

        let mut rows: Vec<Row> = obj_vec_to_iter.iter()
            .map(|row| {

                let cell_fields_formatted = LinearIssueOpInterface::cell_fields_from_row(row, widths);

                max_seen_row_size = max(get_row_height(&cell_fields_formatted), max_seen_row_size);

                let mut cells: Vec<Cell> = cell_fields_formatted
                    .iter()
                    .map(|c| Cell::from(c.clone()))
                    .collect();

                // gen relevant cell colored & replace uncolored edition with colored
                match row {
                    IssueRelatableObject::WorkflowState(state) => {
        
                        let name: String = cell_fields_formatted[0].clone();
                        let color = state.color.clone();
        
                        // Insert new "name" cell, and remove unformatted version
                        cells.insert(0, colored_cell(name, &color));
                        cells.remove(1);
                    },
                    IssueRelatableObject::Assignee(_assignee) => {
                        // No colored cell for users
                    },
                    IssueRelatableObject::Project(project) => {
                        let name: String = cell_fields_formatted[0].clone();
                        let color = project.color.clone();
        
                        // Insert new "name" cell, and remove unformatted version
                        cells.insert(0, colored_cell(name, color.as_deref().unwrap_or("")));
                        cells.remove(1);
                    },
                    IssueRelatableObject::Cycle(_cycle) => {
                        // No colored cell for cycles
                    },
                };
    

                Row::new(cells)
                    .bottom_margin(bottom_margin)
                    .style(Style::default().fg(colors::ISSUE_MODIFICATION_TABLE_TITLE))
            })
            .collect();

        // Set all row heights to max_seen_row_size
        rows = rows.into_iter()
            .map(|row| {
                row.height(max_seen_row_size as u16)
            })
            .collect();

        let table_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if table_style.highlight_table { Color::Yellow } else { Color::White }))
            .title( gen_table_title_spans(table_style) );

        let t = Table::new(rows)
            .header(header)
            .block(table_block)
            .highlight_style(selected_style);

        Ok(t)
    }
}



impl Default for LinearIssueOpInterface {
    fn default() -> LinearIssueOpInterface {
        LinearIssueOpInterface {
            current_op: None,
            selected_idx: None,
            data_state: TableState::default(),
            loading: Arc::new(AtomicBool::new(false)),
            cursor: Arc::new(Mutex::new(GraphQLCursor::default())),

            obj_data: Arc::new(Mutex::new(ModificationOpData::default())),
        }
    }
}