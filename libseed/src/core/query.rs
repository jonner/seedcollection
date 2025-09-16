//! utilities related to database queries
//!
use serde::{
    Deserialize, Serialize,
    de::{IntoDeserializer, value},
};
use std::{ops::Deref, str::FromStr, sync::Arc};
use strum_macros::EnumIter;

pub mod filter {
    use super::DynFilterPart;

    /// An operator for combining filter parts to form a more complex filter expression
    #[derive(Clone)]
    pub enum Op {
        Or,
        And,
    }

    #[derive(Clone)]
    /// An object that allows you easily build compound filters that can be applied to SQL queries
    pub struct CompoundFilterBuilder {
        pub(crate) top: CompoundFilter,
    }

    pub fn and() -> CompoundFilterBuilder {
        CompoundFilterBuilder::new(Op::And)
    }

    pub fn or() -> CompoundFilterBuilder {
        CompoundFilterBuilder::new(Op::Or)
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
            self.top.into()
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
        pub(crate) conditions: Vec<DynFilterPart>,
        pub(crate) op: Op,
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
        NotGreaterThan,
        NotLessThan,
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
                Cmp::NotGreaterThan => write!(f, " <= "),
                Cmp::NotLessThan => write!(f, " >= "),
            }
        }
    }
}

/// A type for specifying the number of rows to return for an SQL query
#[derive(Debug, PartialEq)]
pub struct LimitSpec {
    /// The number of items to return
    pub count: i32,
    /// An optional offset of rows to return. For example, if this value is
    /// `Some(10)`, it means to start returning items starting with the 10th
    /// row.
    pub offset: Option<i32>,
}

impl From<i32> for LimitSpec {
    fn from(count: i32) -> Self {
        LimitSpec {
            count,
            offset: None,
        }
    }
}

impl ToSql for LimitSpec {
    fn to_sql(&self) -> String {
        match self.offset {
            None => format!("LIMIT {}", self.count),
            Some(offset) => format!("LIMIT {} OFFSET {offset}", self.count),
        }
    }
}

/// A type for specifying the sort order of an SQL query
#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, EnumIter, PartialEq)]
pub enum SortOrder {
    #[serde(rename = "asc")]
    #[default]
    Ascending,
    #[serde(rename = "desc")]
    Descending,
}

impl std::fmt::Display for SortOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SortOrder::Ascending => write!(f, "Ascending"),
            SortOrder::Descending => write!(f, "Descending"),
        }
    }
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

/// A type representing an ordered list of multiple sort specifications. The
/// purpose of this type is merely to facilitate various convienience conversion
/// functions by implementing [From]
pub struct SortSpecs<T: ToSql>(pub Vec<SortSpec<T>>);

impl<T: ToSql> From<SortSpec<T>> for SortSpecs<T> {
    fn from(value: SortSpec<T>) -> Self {
        SortSpecs(vec![value])
    }
}

impl<T: ToSql> ToSql for SortSpecs<T> {
    fn to_sql(&self) -> String {
        "ORDER BY ".to_string()
            + &self
                .0
                .iter()
                .map(ToSql::to_sql)
                .collect::<Vec<String>>()
                .join(", ")
    }
}

impl<T: ToSql> From<T> for SortSpecs<T> {
    fn from(value: T) -> Self {
        SortSpecs(vec![SortSpec {
            field: value,
            order: SortOrder::default(),
        }])
    }
}

impl<T: ToSql> From<Vec<T>> for SortSpecs<T> {
    fn from(value: Vec<T>) -> Self {
        Self(
            value
                .into_iter()
                .map(|field| SortSpec::new(field, SortOrder::default()))
                .collect(),
        )
    }
}

#[derive(Clone)]
pub struct DynFilterPart(Arc<dyn filter::FilterPart + Sync>);

impl Deref for DynFilterPart {
    type Target = Arc<dyn filter::FilterPart + Sync>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<F> From<F> for DynFilterPart
where
    F: filter::FilterPart + Send + Sync + 'static,
{
    fn from(value: F) -> Self {
        DynFilterPart(Arc::new(value))
    }
}

#[cfg(test)]
mod tests {
    use super::filter::FilterPart;
    use super::*;

    // Mock ToSql implementation for testing
    #[derive(Clone, Debug, PartialEq)]
    struct MockSortField(String);

    impl ToSql for MockSortField {
        fn to_sql(&self) -> String {
            self.0.clone()
        }
    }

    // Mock FilterPart for testing
    #[derive(Clone)]
    struct MockFilter {
        sql: String,
    }

    impl filter::FilterPart for MockFilter {
        fn add_to_query(&self, builder: &mut sqlx::QueryBuilder<sqlx::Sqlite>) {
            builder.push(&self.sql);
        }
    }

    #[test]
    fn test_limit_spec_to_sql() {
        let limit = LimitSpec {
            count: 10,
            offset: None,
        };
        assert_eq!(limit.to_sql(), "LIMIT 10");

        let limit_with_offset = LimitSpec {
            count: 5,
            offset: Some(20),
        };
        assert_eq!(limit_with_offset.to_sql(), "LIMIT 5 OFFSET 20");
    }

    #[test]
    fn test_sort_order_default() {
        let order = SortOrder::default();
        assert_eq!(order, SortOrder::Ascending);
    }

    #[test]
    fn test_sort_order_from_str() {
        assert_eq!(SortOrder::from_str("asc").unwrap(), SortOrder::Ascending);
        assert_eq!(SortOrder::from_str("desc").unwrap(), SortOrder::Descending);

        let invalid_result = SortOrder::from_str("invalid");
        assert!(invalid_result.is_err());
    }

    #[test]
    fn test_sort_order_to_sql() {
        assert_eq!(SortOrder::Ascending.to_sql(), "ASC");
        assert_eq!(SortOrder::Descending.to_sql(), "DESC");
    }

    #[test]
    fn test_sort_spec_to_sql() {
        let spec = SortSpec::new(MockSortField("name".to_string()), SortOrder::Ascending);
        assert_eq!(spec.to_sql(), "name ASC");

        let spec_desc = SortSpec::new(
            MockSortField("created_at".to_string()),
            SortOrder::Descending,
        );
        assert_eq!(spec_desc.to_sql(), "created_at DESC");
    }

    #[test]
    fn test_sort_specs_to_sql() {
        let specs = SortSpecs(vec![
            SortSpec::new(MockSortField("name".to_string()), SortOrder::Ascending),
            SortSpec::new(
                MockSortField("created_at".to_string()),
                SortOrder::Descending,
            ),
        ]);
        assert_eq!(specs.to_sql(), "ORDER BY name ASC, created_at DESC");
    }

    #[test]
    fn test_compound_filter_builder_build() {
        let mock_filter = MockFilter {
            sql: "test = 1".to_string(),
        };
        let filter_part = filter::and().push(mock_filter).build();

        // Test that we can add it to a query
        let mut builder = sqlx::QueryBuilder::new("SELECT * FROM test WHERE");
        filter_part.add_to_query(&mut builder);
        let sql = builder.sql();
        assert_eq!(sql, "SELECT * FROM test WHERE (test = 1)");
    }

    #[test]
    fn test_compound_filter_add_to_query_empty() {
        let filter = filter::CompoundFilter::new(filter::Op::And);
        let mut builder = sqlx::QueryBuilder::new("SELECT * WHERE");
        builder.push(" ");
        filter.add_to_query(&mut builder);
        let sql = builder.sql();
        assert_eq!(sql, "SELECT * WHERE TRUE");
    }

    #[test]
    fn test_compound_filter_add_to_query_single_condition() {
        let mut filter = filter::CompoundFilter::new(filter::Op::And);
        let mock_filter = MockFilter {
            sql: "name = 'test'".to_string(),
        };
        filter.add_filter(mock_filter.into());

        let mut builder = sqlx::QueryBuilder::new("SELECT * WHERE");
        filter.add_to_query(&mut builder);
        let sql = builder.sql();
        assert_eq!(sql, "SELECT * WHERE (name = 'test')");
    }

    #[test]
    fn test_compound_filter_add_to_query_multiple_and() {
        let mut filter = filter::CompoundFilter::new(filter::Op::And);
        filter.add_filter(
            MockFilter {
                sql: "name = 'test'".to_string(),
            }
            .into(),
        );
        filter.add_filter(
            MockFilter {
                sql: "age > 18".to_string(),
            }
            .into(),
        );

        let mut builder = sqlx::QueryBuilder::new("SELECT * WHERE");
        filter.add_to_query(&mut builder);
        let sql = builder.sql();
        assert_eq!(sql, "SELECT * WHERE (name = 'test' AND age > 18)");
    }

    #[test]
    fn test_compound_filter_add_to_query_multiple_or() {
        let mut filter = filter::CompoundFilter::new(filter::Op::Or);
        filter.add_filter(
            MockFilter {
                sql: "name = 'test'".to_string(),
            }
            .into(),
        );
        filter.add_filter(
            MockFilter {
                sql: "name = 'demo'".to_string(),
            }
            .into(),
        );

        let mut builder = sqlx::QueryBuilder::new("SELECT * WHERE");
        filter.add_to_query(&mut builder);
        let sql = builder.sql();
        assert_eq!(sql, "SELECT * WHERE (name = 'test' OR name = 'demo')");
    }
}
