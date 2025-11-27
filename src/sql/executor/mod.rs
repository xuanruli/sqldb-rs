use crate::error::Result;
use super::{plan::Node, types::Row};
use schema::CreateTable;
use mutation::Insert;
use query::Scan;

mod schema;
mod mutation;
mod query;

pub trait Executor {
    fn execute(&self) -> Result<ResultSet>;
}

impl dyn Executor {
    pub fn build(node: Node) -> Box<dyn Executor> {
        match node {
            Node::CreateTable { schema } => CreateTable::new(schema),
            Node::Insert { table_name, columns, values } => Insert::new(table_name, columns, values),
            Node::Scan { table_name } => Scan::new(table_name),
        }
    }
}

pub enum ResultSet {
    CreateTable {
        create_table: String,
    },
    Insert {
        count: usize,
    },
    Select {
        columns: Vec<String>,
        rows: Vec<Row>,
    },
}