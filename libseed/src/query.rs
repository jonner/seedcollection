//! utilities related to database queries
//!
use serde::{
    de::{value, IntoDeserializer},
    Deserialize, Serialize,
};
use std::{str::FromStr, sync::Arc};

/// An operator for combining filter parts to form a more complex filter expression
#[derive(Clone)]
pub enum Op {
    Or,
    And,
}

#[derive(Clone)]
/// An object that allows you easily build compound filters that can be applied to SQL queries
pub struct CompoundFilterBuilder {
    top: CompoundFilter,
}

impl CompoundFilterBuilder {
    /// Create a new [CompoundFilterBuilder] object that will combine all filter
    /// expressions using the given operator
    pub fn new(op: Op) -> Self {
        Self {
            top: CompoundFilter::new(op),
        }
    }

    /// Add a new filter expression to this compound filter. It will be combined
    /// with all existing filter expressions using the operator that was specified in
    /// the constructor.
    pub fn push<F: Into<DynFilterPart>>(mut self, filter: F) -> Self {
        self.top.add_filter(filter.into());
        self
    }

    /// Generate a new [CompoundFilter] object from this builder object
    pub fn build(self) -> DynFilterPart {
        Arc::new(self.top)
    }
}

/// A Trait implemented by anything that can be a filter. It could be a single field or a
/// multi-level compound filter condition.
pub trait FilterPart: Send {
    /// convert the given filter part to SQL syntax and add it to the given [sqlx::QueryBuilder] object
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
    /// Create a new compound filter object
    pub fn new(op: Op) -> Self {
        Self {
            conditions: Default::default(),
            op,
        }
    }

    /// Create an builder object that is used for building compound filters
    pub fn builder(op: Op) -> CompoundFilterBuilder {
        CompoundFilterBuilder::new(op)
    }

    /// Add a new filter expression to the current filter. It will be combined
    /// with the operator [Op] that was specified in [CompoundFilter::new()]
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
/// An object representing the comparison operator that is used in a filter expression
pub enum Cmp {
    Equal,
    NotEqual,
    Like,
    LessThan,
    GreaterThan,
    LessThanEqual,
    GreatherThanEqual,
    NumericPrefix,
}

impl std::fmt::Display for Cmp {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Cmp::Equal => write!(f, " IS "),
            Cmp::NotEqual => write!(f, " IS NOT "),
            Cmp::NumericPrefix | Cmp::Like => write!(f, " LIKE "),
            Cmp::LessThan => write!(f, " < "),
            Cmp::GreaterThan => write!(f, " != "),
            Cmp::LessThanEqual => write!(f, " <= "),
            Cmp::GreatherThanEqual => write!(f, " >= "),
        }
    }
}

/// A type for specifying the number of rows to return for an SQL query
pub struct LimitSpec(
    /// The number of items to return
    pub i32,
    /// An optional offset of rows to return. For example, if this value is
    /// `Some(10)`, it means to start returning items starting with the 10th
    /// row.
    pub Option<i32>,
);

/// A type for specifying the sort order of an SQL query
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
        Deserialize::deserialize(deserializer)
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

/// a trait that generates an sql respresentation of the implementing type
pub trait ToSql {
    fn to_sql(&self) -> String;
}

/// A type for specifying how the results from an SQL query should be sorted
#[derive(Clone, Debug)]
pub struct SortSpec<T: ToSql> {
    /// The field that the sql query should be sorted on. The type must be
    /// convertible to an SQL representation via [ToSql]
    pub field: T,
    /// The direction to sort results
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

/// A type representing an ordered list of multiple sort specifications.
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
