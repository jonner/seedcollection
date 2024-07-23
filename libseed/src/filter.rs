//! utilities for filtering database queries for the various objects
//!
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone)]
pub enum Op {
    Or,
    And,
}

#[derive(Clone)]
/// An object that allows you easily build complound filters that can be applied to SQL queries
pub struct CompoundFilterBuilder {
    top: CompoundFilter,
}

impl CompoundFilterBuilder {
    pub fn new(op: Op) -> Self {
        Self {
            top: CompoundFilter::new(op),
        }
    }

    pub fn push<F: Into<DynFilterPart>>(mut self, filter: F) -> Self {
        self.top.add_filter(filter.into());
        self
    }

    pub fn build(self) -> DynFilterPart {
        Arc::new(self.top)
    }
}

/// A Trait implemented by anything that can be a filter. It could be a single field or a
/// multi-level compound filter condition.
pub trait FilterPart: Send {
    fn add_to_query(&self, builder: &mut sqlx::QueryBuilder<sqlx::Sqlite>);
}

#[derive(Clone)]
/// An object that represents one or more filter conditions that are combined by a single logical
/// operator ([FilterOp]). Multiple compound filters can be combined together into larger filter
/// conditions
pub struct CompoundFilter {
    conditions: Vec<DynFilterPart>,
    op: Op,
}

impl CompoundFilter {
    pub fn new(op: Op) -> Self {
        Self {
            conditions: Default::default(),
            op,
        }
    }

    pub fn build(op: Op) -> CompoundFilterBuilder {
        CompoundFilterBuilder::new(op)
    }

    pub fn add_filter(&mut self, filter: DynFilterPart) {
        self.conditions.push(filter);
    }
}

impl FilterPart for CompoundFilter {
    fn add_to_query(&self, builder: &mut sqlx::QueryBuilder<sqlx::Sqlite>) {
        if self.conditions.is_empty() {
            builder.push("TRUE");
            return;
        }

        let mut first = true;
        builder.push(" (");
        let separator = match self.op {
            Op::And => " AND ",
            Op::Or => " OR ",
        };

        for cond in &self.conditions {
            if first {
                first = false;
            } else {
                builder.push(separator);
            }
            cond.add_to_query(builder);
        }
        builder.push(")");
    }
}

#[derive(Clone)]
/// An object representing the operation that is used to compare against a filter condition
pub enum Cmp {
    Equal,
    NotEqual,
    Like,
    LessThan,
    GreaterThan,
    LessThanEqual,
    GreatherThanEqual,
}

impl std::fmt::Display for Cmp {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Cmp::Equal => write!(f, " IS "),
            Cmp::NotEqual => write!(f, " IS NOT "),
            Cmp::Like => write!(f, " LIKE "),
            Cmp::LessThan => write!(f, " < "),
            Cmp::GreaterThan => write!(f, " != "),
            Cmp::LessThanEqual => write!(f, " <= "),
            Cmp::GreatherThanEqual => write!(f, " >= "),
        }
    }
}

/// An object that allows you to specify the limit and offset for an SQL query
pub struct LimitSpec(pub i32, pub Option<i32>);

#[derive(Deserialize, Serialize, Clone)]
pub enum SortOrder {
    #[serde(rename = "asc")]
    Ascending,
    #[serde(rename = "desc")]
    Descending,
}

/// An object that allows you to specify the sort for an SQL query
pub struct SortSpec<T: ToString> {
    pub field: T,
    pub order: SortOrder,
}

impl<T: ToString> SortSpec<T> {
    pub fn new(field: T, order: SortOrder) -> Self {
        Self { field, order }
    }
}

pub type DynFilterPart = Arc<dyn FilterPart + Sync>;
