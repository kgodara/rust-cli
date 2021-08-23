use tui::{
    layout::{ Constraint },
    style::{Color, Modifier, Style},
    text::{Span, Spans },
    widgets::{Block, Borders, Cell, Row, Table, TableState},
};

use std::sync::{Arc, Mutex};
use serde_json::json;
use serde_json::Value;

// use colorsys::Color as CTColor;

use crate::util::ui::{ TableStyle, style_color_from_hex_str, gen_table_title_spans };
use crate::util::colors::{ API_REQ_NUM };
use crate::constants::table_columns::{ VIEW_PANEL_COLUMNS };

use crate::util::layout::{ format_str_with_wrap };

pub struct LinearIssueDisplay {
    pub issue_table_data: Arc<Mutex<Option<Value>>>,
    pub issue_table_state: TableState,
}

impl LinearIssueDisplay {

    pub fn get_rendered_issue_data<'a>(table_data: &[Value], widths: &[Constraint], table_style: TableStyle) -> Result<Table<'a>, &'static str> {

        let bottom_margin = table_style.row_bottom_margin.unwrap_or(0);

        let selected_style = Style::default().add_modifier(Modifier::REVERSED);
        let normal_style = Style::default().bg(Color::DarkGray);

        let header_cells: Vec<Cell> = VIEW_PANEL_COLUMNS
            .iter()
            .map(|h| Cell::from(&*h.label).style(Style::default().fg(Color::LightGreen)))
            .collect();

        let header = Row::new(header_cells)
            .style(normal_style)
            .height(1)
            .bottom_margin(1);

        // info!("Header: {:?}", header);

        let mut max_seen_row_size: usize = 0;

        let rows = table_data.iter().map(|row| {

            // info!("Table Row Raw: {:?}", row);

            let cell_fields: Vec<String> = vec![row["number"].clone(),
                    row["title"].clone(),
                    row["state"]["name"].clone(),
                    row["description"].clone(),
                    row["createdAt"].clone()
                ]
                .iter()
                .map(|field| match field {

                    Value::String(x) => x.clone(),
                    Value::Number(x) => x.clone().as_i64().unwrap_or(0).to_string(),
                    Value::Null => String::default(),
                    
                    _ => { String::default() },
                })
                .collect();

            // Get the formatted Strings for each cell field
            let cell_fields_formatted: Vec<String> = cell_fields.iter()
                .enumerate()
                .map(|(idx, cell_field)| {
                    if let Constraint::Length(width_num) = widths[idx] {
                        format_str_with_wrap(cell_field, width_num, VIEW_PANEL_COLUMNS[idx].max_height)
                    } else {
                        error!("get_rendered_issue_data - Constraint must be Constraint::Length: {:?}", widths[idx]);
                        panic!("get_rendered_issue_data - Constraint must be Constraint::Length: {:?}", widths[idx]);
                    }
                })
                .collect();


            // info!("Cell Fields: {:?}", cell_fields);


            let mut current_row_height = cell_fields_formatted
                .iter()
                .map(|content| content.chars().filter(|c| *c == '\n').count())
                .max()
                .unwrap_or(0)
                + 1;
                
            // Ensure that every row is as high as the largest table row
            if current_row_height > max_seen_row_size {
                max_seen_row_size = current_row_height;
            } else {
                current_row_height = max_seen_row_size;
            }

            // info!("Height: {:?}", height);

            let mut cells: Vec<Cell> = cell_fields_formatted.iter().map(|c| Cell::from(c.clone())).collect();

            let generate_state_cell = || {
                // let state_obj = row["state"].clone();
                let name = cell_fields_formatted[2].clone();
                let color = row["state"]["color"].clone();

                let style_color = style_color_from_hex_str(&color);

                match style_color {
                    Some(y) => { Cell::from(name).style(Style::default().fg(y)) },
                    None => Cell::from(String::default()),
                }
            };

            // Insert new "state" cell, and remove unformatted version
            cells.insert(2, generate_state_cell());
            cells.remove(3);

            Row::new(cells).height(current_row_height as u16).bottom_margin(bottom_margin)
        });

        let table_block = Block::default()
                                    .borders(Borders::ALL)
                                    .border_style(Style::default().fg(if table_style.highlight_table { Color::Yellow } else { Color::White }))
                                    .title( gen_table_title_spans(table_style) );

        let t = Table::new(rows)
            .header(header)
            .block(table_block)
            .highlight_style(selected_style)
            .widths(&[
                Constraint::Percentage(10),
                Constraint::Percentage(15),
                Constraint::Percentage(25),
                Constraint::Percentage(20),
                Constraint::Percentage(20)
            ]);
        
        return Ok(t);

    }





}

impl Default for LinearIssueDisplay {

    fn default() -> LinearIssueDisplay {
        LinearIssueDisplay {
            issue_table_data: Arc::new(Mutex::new(Some(Value::Array(vec![])))),
            issue_table_state: TableState::default(),
        }
    }
}
