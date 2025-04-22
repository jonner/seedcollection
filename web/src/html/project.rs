use crate::{
    TemplateKey,
    auth::SqliteUser,
    error::Error,
    state::AppState,
    util::{
        AccessControlled, FlashMessage, Paginator, app_url,
        extract::{Form, Query},
    },
};
use axum::{
    Router,
    extract::{OriginalUri, Path, State},
    http::HeaderMap,
    response::IntoResponse,
    routing::get,
};
use axum_extra::extract::OptionalQuery;
use axum_template::RenderHtml;
use libseed::{
    core::{
        loadable::Loadable,
        query::{
            SortOrder, SortSpec,
            filter::{Cmp, and, or},
        },
    },
    empty_string_as_none,
    project::{
        self, Project,
        allocation::{self, SortField, taxon_name_like},
    },
    sample::{self, Sample},
};
use minijinja::context;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use strum::IntoEnumIterator;
use tracing::{debug, trace, warn};

use super::{
    FilterSortOption, FilterSortSpec, flash_message,
    sample::{SampleFilterParams, sample_filter_spec},
    sort_dirs,
};

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/new", get(show_new_project).post(insert_project))
        .route("/list", get(list_projects))
        .route(
            "/{id}",
            get(show_project).put(modify_project).delete(delete_project),
        )
        .route("/{id}/edit", get(show_project))
        .route("/{id}/add", get(show_add_sample).post(add_sample))
        .nest("/{id}/sample/", super::allocation::router())
}

#[derive(Debug, Deserialize, Serialize)]
struct ProjectListParams {
    #[serde(deserialize_with = "empty_string_as_none")]
    filter: Option<String>,
    page: Option<u32>,
}

async fn list_projects(
    mut user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    OptionalQuery(params): OptionalQuery<ProjectListParams>,
    headers: HeaderMap,
    uri: OriginalUri,
) -> Result<impl IntoResponse, Error> {
    trace!(?params, "Listing projects");
    let mut fbuilder = and().push(project::Filter::User(user.id));
    let (namefilter, page) = match params {
        Some(x) => (
            x.filter.map(|filterstring| {
                debug!(?filterstring, "Got project filter");
                or().push(project::Filter::Name(Cmp::Like, filterstring.clone()))
                    .push(project::Filter::Description(
                        Cmp::Like,
                        filterstring.clone(),
                    ))
                    .build()
            }),
            x.page,
        ),
        None => (None, None),
    };
    if let Some(namefilter) = namefilter {
        fbuilder = fbuilder.push(namefilter);
    }
    let filter = fbuilder.build();
    let paginator = Paginator::new(
        Project::count(Some(filter.clone()), &state.db).await? as u32,
        user.preferences(&state.db).await?.pagesize.into(),
        page,
    );
    let projects =
        Project::load_all(Some(filter), None, Some(paginator.limits()), &state.db).await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 projects => projects,
                 summary => paginator,
                 request_uri => uri.to_string(),
                 filteronly => headers.get("HX-Request").is_some()),
    ))
}

async fn show_new_project(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, Error> {
    Ok(RenderHtml(key, state.tmpl.clone(), context!(user => user)))
}

#[derive(Debug, Deserialize, Serialize)]
struct ProjectParams {
    name: String,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    description: Option<String>,
}

async fn insert_project(
    user: SqliteUser,
    State(state): State<AppState>,
    Form(params): Form<ProjectParams>,
) -> Result<impl IntoResponse, Error> {
    if params.name.is_empty() {
        return Err(Error::RequiredParameterMissing("name".into()));
    }
    let mut project = Project::new(
        params.name.clone(),
        params.description.as_ref().cloned(),
        user.id,
    );
    project.insert(&state.db).await?;

    debug!(project.id, "successfully inserted project");
    let projecturl = app_url(&format!("/project/{}", project.id));

    Ok((
        [("HX-Redirect", projecturl)],
        flash_message(
            state,
            FlashMessage::Success(format!(
                r#"Added new project {}: {} to the database"#,
                project.id, params.name
            )),
        ),
    ))
}

#[derive(Deserialize, Serialize)]
struct ShowProjectQueryParams {
    sort: Option<SortField>,
    dir: Option<SortOrder>,
    filter: Option<String>,
    page: Option<u32>,
}

async fn show_project(
    mut user: SqliteUser,
    TemplateKey(key): TemplateKey,
    Path(id): Path<<Project as Loadable>::Id>,
    State(state): State<AppState>,
    Query(mut params): Query<ShowProjectQueryParams>,
    headers: HeaderMap,
    uri: OriginalUri,
) -> Result<impl IntoResponse, Error> {
    let mut project = Project::load_for_user(id, &user, &state.db).await?;
    let field = params.sort.get_or_insert(SortField::Taxon);
    let dir = params.dir.get_or_insert_default();
    let sort = SortSpec::new(field.clone(), *dir);
    let sample_filter = match params.filter {
        Some(ref fragment) if !fragment.trim().is_empty() => Some(
            or().push(taxon_name_like(fragment))
                .push(allocation::Filter::SourceName(Cmp::Like, fragment.clone()))
                .push(allocation::Filter::Notes(Cmp::Like, fragment.clone()))
                .build(),
        ),
        _ => None,
    };
    let paginator = Paginator::new(
        project
            .count_samples(sample_filter.clone(), &state.db)
            .await? as u32,
        user.preferences(&state.db).await?.pagesize.into(),
        params.page,
    );
    project
        .load_samples(
            sample_filter,
            Some(sort.into()),
            Some(paginator.limits()),
            &state.db,
        )
        .await?;

    let filter_spec: FilterSortSpec<SortField> = FilterSortSpec {
        filter: params.filter.unwrap_or_default(),
        sort_fields: SortField::iter()
            .map(|opt| FilterSortOption {
                value: opt.clone(),
                name: opt.to_string(),
                selected: &opt == field,
            })
            .collect::<Vec<_>>(),
        sort_dirs: sort_dirs(params.dir.unwrap_or_default()),
        additional_filters: vec![],
    };

    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 project => project,
                 filter_spec => filter_spec,
                 summary => paginator,
                 request_uri => uri.to_string(),
                 filteronly => headers.get("HX-Request").is_some()),
    ))
}

async fn modify_project(
    user: SqliteUser,
    Path(id): Path<<Project as Loadable>::Id>,
    State(state): State<AppState>,
    Form(params): Form<ProjectParams>,
) -> Result<impl IntoResponse, Error> {
    let mut project = Project::load_for_user(id, &user, &state.db).await?;

    project.name.clone_from(&params.name);
    project.description.clone_from(&params.description);
    project.update(&state.db).await.map_err(|e| match e {
        libseed::Error::InvalidStateMissingAttribute(attr) => Error::RequiredParameterMissing(attr),
        _ => e.into(),
    })?;

    Ok((
        [("HX-Redirect", app_url(&format!("/project/{id}")))],
        flash_message(
            state,
            FlashMessage::Success("Successfully updated project".to_string()),
        ),
    ))
}

async fn delete_project(
    user: SqliteUser,
    Path(id): Path<<Project as Loadable>::Id>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, Error> {
    let mut project = Project::load_for_user(id, &user, &state.db).await?;
    project.delete(&state.db).await?;
    Ok((
        [("HX-Redirect", app_url("/project/list"))],
        flash_message(
            state,
            FlashMessage::Success(format!("Deleted project '{id}'")),
        ),
    ))
}

#[derive(Debug, Deserialize, Serialize)]
struct AddSampleParams {
    #[serde(flatten)]
    filter: SampleFilterParams,
}
async fn show_add_sample(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path(id): Path<<Project as Loadable>::Id>,
    headers: HeaderMap,
    Query(mut params): Query<AddSampleParams>,
) -> Result<impl IntoResponse, Error> {
    let project = Project::load(id, &state.db).await?;
    let ids_in_project = sqlx::query!(
        "SELECT PS.sampleid from sc_project_samples PS WHERE PS.projectid=?",
        id
    )
    .fetch_all(state.db.pool())
    .await?;
    let ids = ids_in_project.iter().map(|row| row.sampleid).collect();
    let mut filterbuilder = and()
        .push(sample::Filter::IdNotIn(ids))
        .push(sample::Filter::UserId(user.id));
    if !params.filter.all {
        filterbuilder = filterbuilder.push(sample::Filter::Quantity(Cmp::NotEqual, 0.0))
    }
    if let Some(filterstring) = &params.filter.filter {
        let search_filter = or()
            .push(sample::taxon_name_like(filterstring))
            .push(sample::Filter::Notes(Cmp::Like, filterstring.clone()))
            .push(sample::Filter::SourceName(Cmp::Like, filterstring.clone()))
            .build();
        filterbuilder = filterbuilder.push(search_filter);
    }
    let sort = params
        .filter
        .sort
        .get_or_insert(sample::SortField::TaxonSequence);
    let dir = params.filter.dir.get_or_insert(SortOrder::Ascending);

    let samples = Sample::load_all(
        Some(filterbuilder.build()),
        Some(
            SortSpec {
                field: sort.clone(),
                order: *dir,
            }
            .into(),
        ),
        None,
        &state.db,
    )
    .await?;

    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
            project => project,
            samples => samples,
            refresh_samples => headers.get("hx-request").is_some(),
            filter_spec => sample_filter_spec(&params.filter),
            query => params,
        ),
    ))
}

async fn add_sample(
    user: SqliteUser,
    State(state): State<AppState>,
    Path(id): Path<<Project as Loadable>::Id>,
    Form(params): Form<Vec<(String, String)>>,
) -> Result<impl IntoResponse, Error> {
    let mut project = Project::load_for_user(id, &user, &state.db).await?;
    if params.is_empty() {
        return Err(Error::RequiredParameterMissing("samples".into()));
    }
    let toadd: HashSet<<Sample as Loadable>::Id> = params
        .iter()
        .filter_map(|(name, value)| match name.as_str() {
            "sample" => value.parse::<<Sample as Loadable>::Id>().ok(),
            _ => None,
        })
        .collect();
    let mut fb = or();
    for id in &toadd {
        fb = fb.push(sample::Filter::Id(Cmp::Equal, *id));
    }
    fb = and().push(fb.build()).push(sample::Filter::UserId(user.id));
    let valid_samples = Sample::load_all(Some(fb.build()), None, None, &state.db).await?;

    let mut n_inserted = 0;
    for sample in valid_samples {
        let id = sample.id;
        match project.allocate_sample(sample, &state.db).await {
            Ok(_) => n_inserted += 1,
            Err(e) => warn!("Failed to add sample {id} to the project: {e}"),
        }
    }

    let n_dropped = toadd.len() - n_inserted;
    if n_inserted == 0 {
        Err(Error::OperationFailed(
            "No samples added to this project".to_string(),
        ))
    } else {
        let message: FlashMessage = if n_dropped > 0 {
            FlashMessage::Warning(format!(
                "Added {n_inserted} sample(s) to the project. Failed to add {n_dropped} sample(s) due to errors."
            ))
        } else {
            FlashMessage::Success(format!("Added {n_inserted} samples to this project"))
        };
        Ok((
            [("HX-Trigger", "reload-samples")],
            flash_message(state, message),
        ))
    }
}
