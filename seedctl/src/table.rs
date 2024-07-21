use anyhow::Result;
use libseed::sample::Sample;
use tabled::Tabled;

#[derive(Tabled)]
pub struct SampleRow {
    id: i64,
    taxon: String,
    source: String,
}

impl SampleRow {
    pub fn new(sample: &Sample) -> Result<Self> {
        Ok(Self {
            id: sample.id,
            taxon: sample.taxon.object()?.complete_name.clone(),
            source: sample.source.object()?.name.clone(),
        })
    }
}

#[derive(Tabled)]
pub struct SampleRowFull {
    id: i64,
    taxon: String,
    source: String,
    date: String,
    #[tabled(display_with = "table_display_option")]
    quantity: Option<i64>,
}

fn table_display_option<T: ToString>(o: &Option<T>) -> String {
    match o {
        Some(v) => v.to_string(),
        None => "".to_string(),
    }
}

impl SampleRowFull {
    pub fn new(sample: &Sample) -> Result<Self> {
        Ok(Self {
            id: sample.id,
            taxon: sample.taxon.object()?.complete_name.clone(),
            source: sample.source.object()?.name.clone(),
            date: match (sample.month, sample.year) {
                (Some(m), Some(y)) => format!("{m}/{y}"),
                (None, Some(y)) => y.to_string(),
                _ => "Unknown".to_string(),
            },
            quantity: sample.quantity,
        })
    }
}
