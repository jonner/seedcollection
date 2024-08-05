use std::sync::Arc;

use anyhow::Result;
use libseed::{
    filter::Cmp,
    project::{allocation, Allocation, Project},
    sample::{self, Certainty, Sample},
    source::Source,
    taxonomy::{Germination, NativeStatus, Rank, Taxon},
    user::User,
};
use sqlx::{Pool, Sqlite};
use tabled::{Table, Tabled};

pub trait SeedctlTable {
    fn styled(&mut self) -> &mut Self;
}

impl SeedctlTable for Table {
    fn styled(&mut self) -> &mut Self {
        use tabled::settings::{object::Segment, width::Width, Modify, Style};
        let m = Modify::new(Segment::all()).with(Width::wrap(60).keep_words());
        self.with(m).with(Style::psql())
    }
}

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

fn table_display_germination(germ: &Option<Vec<Germination>>) -> String {
    germ.as_ref()
        .map(|v| {
            v.iter()
                .map(|g| {
                    format!(
                        "{}: {}",
                        g.code.clone(),
                        g.summary.as_deref().unwrap_or("Unknown")
                    )
                })
                .collect::<Vec<String>>()
                .join("\n")
        })
        .unwrap_or_else(|| "".to_string())
}

fn table_display_allocations(allocations: &Vec<Allocation>) -> String {
    let s = allocations
        .iter()
        .map(|a| format!("{} ({})", a.project.name, a.project.id))
        .collect::<Vec<String>>()
        .join("\n");
    match s.is_empty() {
        true => "Not allocated to any project".to_string(),
        false => s,
    }
}

#[derive(Tabled)]
#[tabled(rename_all = "PascalCase")]
pub struct SampleRowDetails {
    id: i64,
    taxon: String,
    #[tabled(display_with = "format_string_vec", rename = "Common Names")]
    common_names: Vec<String>,
    source: String,
    #[tabled(rename = "Collection Date")]
    date: String,
    #[tabled(display_with = "table_display_option")]
    quantity: Option<i64>,
    certainty: Certainty,
    #[tabled(
        display_with = "table_display_germination",
        rename = "Germination Codes"
    )]
    germination: Option<Vec<Germination>>,
    #[tabled(display_with = "table_display_option")]
    notes: Option<String>,
    #[tabled(display_with = "table_display_allocations")]
    allocations: Vec<Allocation>,
}

impl SampleRowDetails {
    pub async fn new(sample: &mut Sample, pool: &Pool<Sqlite>) -> Result<Self> {
        let taxon = sample.taxon.load_mut(pool).await?;
        taxon.load_germination_info(pool).await?;
        let src = sample.source.object()?;
        let allocations = Allocation::load_all(
            Some(Arc::new(allocation::Filter::SampleId(sample.id))),
            None,
            pool,
        )
        .await?;

        Ok(Self {
            id: sample.id,
            taxon: format!("{} ({})", taxon.complete_name, taxon.id),
            common_names: taxon.vernaculars.clone(),
            source: format!("{} ({})", src.name, src.id),
            date: datestring(sample.month, sample.year),
            quantity: sample.quantity,
            certainty: sample.certainty.clone(),
            germination: taxon.germination.clone(),
            notes: sample.notes.as_ref().cloned(),
            allocations,
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

fn datestring(m: Option<u32>, y: Option<u32>) -> String {
    match (m, y) {
        (Some(m), Some(y)) => format!("{m}/{y}"),
        (None, Some(y)) => y.to_string(),
        _ => "Unknown".to_string(),
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
            date: datestring(sample.month, sample.year),
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
pub struct TaxonRowDetails {
    id: i64,
    name: String,
    #[tabled(display_with = "format_string_vec", rename = "Common Names")]
    common_names: Vec<String>,
    rank: Rank,
    #[tabled(display_with = "table_display_option", rename = "MN Status")]
    mn_status: Option<NativeStatus>,
    #[tabled(
        display_with = "table_display_germination",
        rename = "Germination Codes"
    )]
    germination: Option<Vec<Germination>>,
    #[tabled(display_with = "table_display_samples")]
    samples: Vec<Sample>,
}

fn table_display_samples(samples: &Vec<Sample>) -> String {
    samples
        .iter()
        .map(|s| {
            format!(
                "{}: {} ({})",
                s.id,
                s.source.object().unwrap().name,
                s.year
                    .map(|y| y.to_string())
                    .unwrap_or_else(|| "Unknown date".to_string())
            )
        })
        .collect::<Vec<String>>()
        .join("\n")
}

impl TaxonRowDetails {
    pub async fn new(taxon: &mut Taxon, pool: &Pool<Sqlite>) -> Result<Self> {
        taxon.load_germination_info(pool).await?;
        let mut samples = Sample::load_all(
            Some(sample::Filter::TaxonId(Cmp::Equal, taxon.id).into()),
            None,
            pool,
        )
        .await?;
        for ref mut s in &mut samples {
            s.source.load(pool).await?;
        }

        Ok(Self {
            id: taxon.id,
            rank: taxon.rank.clone(),
            name: taxon.complete_name.clone(),
            common_names: taxon.vernaculars.clone(),
            mn_status: taxon.native_status.clone(),
            germination: taxon.germination.clone(),
            samples,
        })
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

#[derive(Tabled)]
#[tabled(rename_all = "PascalCase")]
pub struct GerminationRow {
    id: i64,
    code: String,
    #[tabled(display_with = "table_display_option")]
    summary: Option<String>,
    #[tabled(display_with = "table_display_option")]
    description: Option<String>,
}

impl GerminationRow {
    pub fn new(g: &Germination) -> Self {
        Self {
            id: g.id.clone(),
            code: g.code.clone(),
            summary: g.summary.clone(),
            description: g.description.clone(),
        }
    }
}
