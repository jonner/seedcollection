pub enum FilterOp {
    Or,
    And,
}

pub trait FilterPart: Send {
    fn add_to_query(&self, builder: &mut sqlx::QueryBuilder<sqlx::Sqlite>);
}

pub struct CompoundFilter {
    conditions: Vec<Box<dyn FilterPart>>,
    op: FilterOp,
}

impl CompoundFilter {
    pub fn new(op: FilterOp) -> Self {
        Self {
            conditions: Default::default(),
            op,
        }
    }

    pub fn add_filter(&mut self, filter: Box<dyn FilterPart>) {
        self.conditions.push(filter);
    }
}

impl FilterPart for CompoundFilter {
    fn add_to_query(&self, builder: &mut sqlx::QueryBuilder<sqlx::Sqlite>) {
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