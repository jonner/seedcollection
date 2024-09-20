//! utilities for filtering database queries for the various objects
//!
use serde::{
    de::{value, IntoDeserializer},
    Deserialize, Serialize,
};
use std::{str::FromStr, sync::Arc};

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
/// operator ([Op]). Multiple compound filters can be combined together into larger filter
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

    pub fn builder(op: Op) -> CompoundFilterBuilder {
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

#[derive(Deserialize, Serialize, Clone, Debug)]
pub enum SortOrder {
    #[serde(rename = "asc")]
    Ascending,
    #[serde(rename = "desc")]
    Descending,
}

impl FromStr for SortOrder {
    type Err = value::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let deserializer = s.into_deserializer();
        let val: Self = Deserialize::deserialize(deserializer)?;
        Ok(val)
    }
}

impl ToSql for SortOrder {
    fn to_sql(&self) -> String {
        match self {
            Self::Ascending => "ASC",
            Self::Descending => "DESC",
        }
        .into()
    }
}

pub trait ToSql {
    fn to_sql(&self) -> String;
}

/// An object that allows you to specify the sort for an SQL query
#[derive(Clone, Debug)]
pub struct SortSpec<T: ToSql> {
    pub field: T,
    pub order: SortOrder,
}

impl<T: ToSql> ToSql for SortSpec<T> {
    fn to_sql(&self) -> String {
        format!("{} {}", self.field.to_sql(), self.order.to_sql())
    }
}

impl<T: ToSql> SortSpec<T> {
    pub fn new(field: T, order: SortOrder) -> Self {
        Self { field, order }
    }
}

pub struct SortSpecs<T: ToSql>(pub Vec<SortSpec<T>>);

impl<T: ToSql> From<SortSpec<T>> for SortSpecs<T> {
    fn from(value: SortSpec<T>) -> Self {
        SortSpecs(vec![value])
    }
}

impl<T: ToSql> ToSql for SortSpecs<T> {
    fn to_sql(&self) -> String {
        self.0
            .iter()
            .map(ToSql::to_sql)
            .collect::<Vec<String>>()
            .join(",")
    }
}

impl<T: ToSql> From<T> for SortSpecs<T> {
    fn from(value: T) -> Self {
        SortSpecs(vec![SortSpec {
            field: value,
            order: SortOrder::Ascending,
        }])
    }
}

impl<T: ToSql> From<Vec<T>> for SortSpecs<T> {
    fn from(mut value: Vec<T>) -> Self {
        Self(
            value
                .drain(..)
                .map(|field| SortSpec::new(field, SortOrder::Ascending))
                .collect(),
        )
    }
}

pub type DynFilterPart = Arc<dyn FilterPart + Sync>;
