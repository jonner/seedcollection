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
use serde::{ser::SerializeSeq, Serialize, Serializer};
use sqlx::{Pool, Sqlite};
use tabled::Tabled;

#[derive(Tabled, Serialize)]
#[tabled(rename_all = "PascalCase")]
pub struct SampleRow {
    id: i64,
    taxon: String,
    source: String,
}

impl SampleRow {
    pub fn new(sample: Sample) -> Result<Self, libseed::Error> {
        Ok(Self {
            id: sample.id,
            taxon: sample.taxon.object()?.complete_name.clone(),
            source: sample.source.object()?.name.clone(),
        })
    }
}

impl TryFrom<Sample> for SampleRow {
    type Error = libseed::Error;

    fn try_from(sample: Sample) -> Result<Self, Self::Error> {
        Self::new(sample)
    }
}

#[derive(Tabled, Serialize)]
#[tabled(rename_all = "PascalCase")]
pub struct SampleRowFull {
    id: i64,
    taxon: String,
    source: String,
    date: String,
    #[tabled(display_with = "table_display_option")]
    quantity: Option<i64>,
}

impl TryFrom<Sample> for SampleRowFull {
    type Error = libseed::Error;

    fn try_from(sample: Sample) -> Result<Self, Self::Error> {
        Self::new(sample)
    }
}

fn table_display_option<T: ToString>(o: &Option<T>) -> String {
    match o {
        Some(v) => v.to_string(),
        None => "".to_string(),
    }
}

impl SampleRowFull {
    pub fn new(sample: Sample) -> Result<Self, libseed::Error> {
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

fn display_germination(germs: &Option<Vec<Germination>>) -> Option<Vec<String>> {
    germs.as_ref().map(|v| {
        v.iter()
            .map(|g| {
                format!(
                    "{}: {}",
                    g.code.clone(),
                    g.summary.as_deref().unwrap_or("Unknown")
                )
            })
            .collect()
    })
}
fn table_display_germination(germ: &Option<Vec<Germination>>) -> String {
    display_germination(germ)
        .map(|val| val.join("\n"))
        .unwrap_or_default()
}

fn serialize_germination<S>(germ: &Option<Vec<Germination>>, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mapped = display_germination(germ);
    match mapped {
        Some(vals) => s.serialize_some(&vals),
        None => s.serialize_none(),
    }
}

fn display_allocations(allocations: &[Allocation]) -> Vec<String> {
    allocations
        .iter()
        .map(|a| format!("{} ({})", a.project.name, a.project.id))
        .collect::<Vec<String>>()
}
fn table_display_allocations(allocations: &[Allocation]) -> String {
    let s = display_allocations(allocations).join("\n");
    match s.is_empty() {
        true => "Not allocated to any project".to_string(),
        false => s,
    }
}
fn serialize_allocations<S>(allocations: &[Allocation], s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let vals = display_allocations(allocations);
    let mut seq = s.serialize_seq(Some(allocations.len()))?;
    for elem in vals {
        seq.serialize_element(&elem)?;
    }
    seq.end()
}

#[derive(Tabled, Serialize)]
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
    #[serde(serialize_with = "serialize_germination")]
    germination: Option<Vec<Germination>>,
    #[tabled(display_with = "table_display_option")]
    notes: Option<String>,
    #[tabled(display_with = "table_display_allocations")]
    #[serde(serialize_with = "serialize_allocations")]
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

#[derive(Tabled, Serialize)]
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

#[derive(Tabled, Serialize)]
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

#[derive(Tabled, Serialize)]
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

#[derive(Tabled, Serialize)]
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

#[derive(Tabled, Serialize)]
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

fn format_string_vec(names: &[String]) -> String {
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

#[derive(Tabled, Serialize)]
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
    #[serde(serialize_with = "serialize_germination")]
    germination: Option<Vec<Germination>>,
    #[tabled(display_with = "table_display_samples")]
    samples: Vec<TaxonSample>,
}

#[derive(Serialize)]
struct TaxonSample {
    id: i64,
    source: String,
    year: Option<u32>,
}

fn table_display_samples(samples: &[TaxonSample]) -> String {
    samples
        .iter()
        .map(|s| {
            format!(
                "{}: {} ({})",
                s.id,
                s.source,
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
            samples: samples
                .iter()
                .map(|s| -> anyhow::Result<TaxonSample> {
                    Ok(TaxonSample {
                        id: s.id,
                        source: s.source.object()?.name.clone(),
                        year: s.year,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

#[derive(Tabled, Serialize)]
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

#[derive(Tabled, Serialize)]
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
            id: g.id,
            code: g.code.clone(),
            summary: g.summary.clone(),
            description: g.description.clone(),
        }
    }
}
