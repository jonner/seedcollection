use anyhow::Result;
use futures::future::try_join_all;
use libseed::{
    core::{database::Database, loadable::Loadable, query::Cmp},
    project::{AllocatedSample, Project, allocation},
    sample::{self, Certainty, Sample},
    source::Source,
    taxonomy::{Germination, NativeStatus, Rank, Taxon},
    user::User,
};
use serde::{Serialize, Serializer, ser::SerializeSeq};
use tabled::Tabled;

#[derive(Tabled, Serialize)]
#[tabled(rename_all = "PascalCase")]
pub(crate) struct SampleRow {
    id: i64,
    taxon: String,
    source: String,
}

impl SampleRow {
    pub(crate) fn new(sample: Sample) -> Result<Self, libseed::Error> {
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
pub(crate) struct SampleRowFull {
    id: i64,
    taxon: String,
    source: String,
    date: String,
    #[tabled(display("tabled::derive::display::option", ""))]
    quantity: Option<f64>,
}

impl TryFrom<Sample> for SampleRowFull {
    type Error = libseed::Error;

    fn try_from(sample: Sample) -> Result<Self, Self::Error> {
        Self::new(sample)
    }
}

impl SampleRowFull {
    pub(crate) fn new(sample: Sample) -> Result<Self, libseed::Error> {
        Ok(Self {
            id: sample.id,
            taxon: sample.taxon_display_name()?,
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

fn display_projects(projects: &[Project]) -> Vec<String> {
    projects
        .iter()
        .map(|p| format!("{} ({})", p.name, p.id))
        .collect::<Vec<String>>()
}

fn table_display_projects(projects: &[Project]) -> String {
    let s = display_projects(projects).join("\n");
    match s.is_empty() {
        true => "Not allocated to any project".to_string(),
        false => s,
    }
}
fn serialize_allocations<S>(projects: &[Project], s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let vals = display_projects(projects);
    let mut seq = s.serialize_seq(Some(projects.len()))?;
    for elem in vals {
        seq.serialize_element(&elem)?;
    }
    seq.end()
}

#[derive(Tabled, Serialize)]
#[tabled(rename_all = "PascalCase")]
pub(crate) struct SampleRowDetails {
    id: i64,
    taxon: String,
    #[tabled(display = "format_string_vec", rename = "Common Names")]
    common_names: Vec<String>,
    source: String,
    #[tabled(rename = "Collection Date")]
    date: String,
    #[tabled(display("tabled::derive::display::option", ""))]
    quantity: Option<f64>,
    certainty: Certainty,
    #[tabled(display("table_display_germination"), rename = "Germination Codes")]
    #[serde(serialize_with = "serialize_germination")]
    germination: Option<Vec<Germination>>,
    #[tabled(display("tabled::derive::display::option", ""))]
    notes: Option<String>,
    #[tabled(display("table_display_projects"))]
    #[serde(serialize_with = "serialize_allocations")]
    projects: Vec<Project>,
}

impl SampleRowDetails {
    pub(crate) async fn new(sample: &mut Sample, db: &Database) -> Result<Self> {
        let taxon = sample.taxon.load_mut(db).await?;
        taxon.load_germination_info(db).await?;
        let src = sample.source.object()?;
        let allocations = AllocatedSample::load_all(
            Some(allocation::Filter::SampleId(sample.id).into()),
            None,
            db,
        )
        .await?;
        let projects = try_join_all(
            allocations
                .into_iter()
                .map(|alloc| Project::load(alloc.projectid, db)),
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
            projects,
        })
    }
}

#[derive(Tabled, Serialize)]
#[tabled(rename_all = "PascalCase")]
pub(crate) struct ProjectRow {
    id: i64,
    name: String,
    #[tabled(display("tabled::derive::display::option", ""))]
    description: Option<String>,
}

impl ProjectRow {
    pub(crate) fn new(project: &Project) -> Self {
        ProjectRow {
            id: project.id,
            name: project.name.clone(),
            description: project.description.as_ref().cloned(),
        }
    }
}

#[derive(Tabled, Serialize)]
#[tabled(rename_all = "PascalCase")]
pub(crate) struct AllocationRow {
    id: i64,
    #[tabled(rename = "Sample ID")]
    sample_id: i64,
    taxon: String,
    source: String,
}

impl AllocationRow {
    pub(crate) fn new(allocation: &AllocatedSample) -> Result<Self> {
        let sample = &allocation.sample;
        Ok(Self {
            id: allocation.id,
            sample_id: sample.id,
            taxon: sample.taxon.object()?.complete_name.clone(),
            source: sample.source.object()?.name.clone(),
        })
    }
}

fn datestring(m: Option<u8>, y: Option<u32>) -> String {
    match (m, y) {
        (Some(m), Some(y)) => format!("{m}/{y}"),
        (None, Some(y)) => y.to_string(),
        _ => "Unknown".to_string(),
    }
}

#[derive(Tabled, Serialize)]
#[tabled(rename_all = "PascalCase")]
pub(crate) struct AllocationRowFull {
    id: i64,
    #[tabled(rename = "Sample ID")]
    sample_id: i64,
    taxon: String,
    source: String,
    date: String,
    #[tabled(display("tabled::derive::display::option", ""))]
    quantity: Option<f64>,
    #[tabled(display("tabled::derive::display::option", ""))]
    notes: Option<String>,
}

impl AllocationRowFull {
    pub(crate) fn new(allocation: &AllocatedSample) -> Result<Self> {
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
pub(crate) struct SourceRowFull {
    id: i64,
    name: String,
    #[tabled(display("tabled::derive::display::option", ""))]
    latitude: Option<f64>,
    #[tabled(display("tabled::derive::display::option", ""))]
    longitude: Option<f64>,
    #[tabled(display("tabled::derive::display::option", ""))]
    description: Option<String>,
}

impl SourceRowFull {
    pub(crate) fn new(source: &Source) -> Self {
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
pub(crate) struct SourceRow {
    id: i64,
    name: String,
}

impl SourceRow {
    pub(crate) fn new(source: &Source) -> Self {
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
pub(crate) struct TaxonRow {
    id: i64,
    rank: Rank,
    name: String,
    #[tabled(display("format_string_vec"), rename = "Common Names")]
    common_names: Vec<String>,
    #[tabled(display("tabled::derive::display::option", ""), rename = "MN Status")]
    mn_status: Option<NativeStatus>,
}

impl TaxonRow {
    pub(crate) fn new(taxon: &Taxon) -> Self {
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
pub(crate) struct TaxonRowDetails {
    id: i64,
    name: String,
    #[tabled(display("format_string_vec"), rename = "Common Names")]
    common_names: Vec<String>,
    rank: Rank,
    #[tabled(display("tabled::derive::display::option", ""), rename = "MN Status")]
    mn_status: Option<NativeStatus>,
    #[tabled(display("table_display_germination"), rename = "Germination Codes")]
    #[serde(serialize_with = "serialize_germination")]
    germination: Option<Vec<Germination>>,
    #[tabled(display("table_display_samples"))]
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
    pub(crate) async fn new(taxon: &mut Taxon, db: &Database) -> Result<Self> {
        taxon.load_germination_info(db).await?;
        let mut samples = Sample::load_all(
            Some(sample::Filter::TaxonId(Cmp::Equal, taxon.id).into()),
            None,
            db,
        )
        .await?;
        for ref mut s in &mut samples {
            s.source.load(db, false).await?;
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
pub(crate) struct UserRow {
    id: i64,
    username: String,
    email: String,
}

impl UserRow {
    pub(crate) fn new(user: &User) -> Self {
        Self {
            id: user.id,
            username: user.username.clone(),
            email: user.email.clone(),
        }
    }
}

#[derive(Tabled, Serialize)]
#[tabled(rename_all = "PascalCase")]
pub(crate) struct GerminationRow {
    id: i64,
    code: String,
    #[tabled(display("tabled::derive::display::option", ""))]
    summary: Option<String>,
    #[tabled(display("tabled::derive::display::option", ""))]
    description: Option<String>,
}

impl GerminationRow {
    pub(crate) fn new(g: &Germination) -> Self {
        Self {
            id: g.id,
            code: g.code.clone(),
            summary: g.summary.clone(),
            description: g.description.clone(),
        }
    }
}
