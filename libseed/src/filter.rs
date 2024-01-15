use std::sync::Arc;

#[derive(Clone)]
pub enum FilterOp {
    Or,
    And,
}

#[derive(Clone)]
pub struct FilterBuilder {
    top: CompoundFilter,
}

impl FilterBuilder {
    pub fn new(op: FilterOp) -> Self {
        Self {
            top: CompoundFilter::new(op),
        }
    }

    pub fn push(mut self, filter: DynFilterPart) -> Self {
        self.top.add_filter(filter);
        self
    }

    pub fn build(self) -> DynFilterPart {
        Arc::new(self.top)
    }
}

pub trait FilterPart: Send {
    fn add_to_query(&self, builder: &mut sqlx::QueryBuilder<sqlx::Sqlite>);
}

#[derive(Clone)]
pub struct CompoundFilter {
    conditions: Vec<DynFilterPart>,
    op: FilterOp,
}

impl CompoundFilter {
    pub fn new(op: FilterOp) -> Self {
        Self {
            conditions: Default::default(),
            op,
        }
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
            FilterOp::And => " AND ",
            FilterOp::Or => " OR ",
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

pub type DynFilterPart = Arc<dyn FilterPart + Sync>;
