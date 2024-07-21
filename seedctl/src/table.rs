use anyhow::Result;
use libseed::{
    project::{Allocation, Project},
    sample::Sample,
    source::Source,
    taxonomy::{NativeStatus, Rank, Taxon},
    user::User,
};
use tabled::Tabled;

#[derive(Tabled)]
#[tabled(rename_all = "PascalCase")]
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
#[tabled(rename_all = "PascalCase")]
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

#[derive(Tabled)]
#[tabled(rename_all = "PascalCase")]
pub struct ProjectRow {
    id: i64,
    name: String,
    #[tabled(display_with = "table_display_option")]
    description: Option<String>,
}

impl ProjectRow {
    pub fn new(project: &Project) -> Self {
        ProjectRow {
            id: project.id,
            name: project.name.clone(),
            description: project.description.as_ref().cloned(),
        }
    }
}

#[derive(Tabled)]
#[tabled(rename_all = "PascalCase")]
pub struct AllocationRow {
    id: i64,
    #[tabled(rename = "Sample ID")]
    sample_id: i64,
    taxon: String,
    source: String,
}

impl AllocationRow {
    pub fn new(allocation: &Allocation) -> Result<Self> {
        let sample = &allocation.sample;
        Ok(Self {
            id: allocation.id,
            sample_id: sample.id,
            taxon: sample.taxon.object()?.complete_name.clone(),
            source: sample.source.object()?.name.clone(),
        })
    }
}

#[derive(Tabled)]
#[tabled(rename_all = "PascalCase")]
pub struct AllocationRowFull {
    id: i64,
    #[tabled(rename = "Sample ID")]
    sample_id: i64,
    taxon: String,
    source: String,
    date: String,
    #[tabled(display_with = "table_display_option")]
    quantity: Option<i64>,
    #[tabled(display_with = "table_display_option")]
    notes: Option<String>,
}

impl AllocationRowFull {
    pub fn new(allocation: &Allocation) -> Result<Self> {
        let sample = &allocation.sample;
        Ok(Self {
            id: allocation.id,
            sample_id: sample.id,
            taxon: sample.taxon.object()?.complete_name.clone(),
            source: sample.source.object()?.name.clone(),
            date: match (sample.month, sample.year) {
                (Some(m), Some(y)) => format!("{m}/{y}"),
                (None, Some(y)) => y.to_string(),
                _ => "Unknown".to_string(),
            },
            quantity: sample.quantity,
            notes: sample.notes.clone(),
        })
    }
}

#[derive(Tabled)]
#[tabled(rename_all = "PascalCase")]
pub struct SourceRowFull {
    id: i64,
    name: String,
    #[tabled(display_with = "table_display_option")]
    latitude: Option<f64>,
    #[tabled(display_with = "table_display_option")]
    longitude: Option<f64>,
    #[tabled(display_with = "table_display_option")]
    description: Option<String>,
}

impl SourceRowFull {
    pub fn new(source: &Source) -> Self {
        Self {
            id: source.id,
            name: source.name.clone(),
            latitude: source.latitude,
            longitude: source.longitude,
            description: source.description.clone(),
        }
    }
}

#[derive(Tabled)]
#[tabled(rename_all = "PascalCase")]
pub struct SourceRow {
    id: i64,
    name: String,
}

impl SourceRow {
    pub fn new(source: &Source) -> Self {
        Self {
            id: source.id,
            name: source.name.clone(),
        }
    }
}

fn format_string_vec(names: &Vec<String>) -> String {
    names.join(",\n")
}

#[derive(Tabled)]
#[tabled(rename_all = "PascalCase")]
pub struct TaxonRow {
    id: i64,
    rank: Rank,
    name: String,
    #[tabled(display_with = "format_string_vec", rename = "Common Names")]
    common_names: Vec<String>,
    #[tabled(display_with = "table_display_option", rename = "MN Status")]
    mn_status: Option<NativeStatus>,
}

impl TaxonRow {
    pub fn new(taxon: &Taxon) -> Self {
        Self {
            id: taxon.id,
            rank: taxon.rank.clone(),
            name: taxon.complete_name.clone(),
            common_names: taxon.vernaculars.clone(),
            mn_status: taxon.native_status.clone(),
        }
    }
}

#[derive(Tabled)]
#[tabled(rename_all = "PascalCase")]
pub struct UserRow {
    id: i64,
    username: String,
    email: String,
}

impl UserRow {
    pub fn new(user: &User) -> Self {
        Self {
            id: user.id,
            username: user.username.clone(),
            email: user.email.clone(),
        }
    }
}