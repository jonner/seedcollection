use crate::{
    TemplateKey,
    auth::SqliteUser,
    error::Error,
    state::AppState,
    util::{
        AccessControlled, FlashMessage, FlashMessageKind, Paginator, app_url,
        extract::{Form, Query},
        format_id_number,
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
use tracing::{debug, trace, warn};

use super::{SortOption, flash_message, flash_messages};

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
    )
    .into_response())
}

async fn show_new_project(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, Error> {
    Ok(RenderHtml(key, state.tmpl.clone(), context!(user => user)).into_response())
}

#[derive(Debug, Deserialize, Serialize)]
struct ProjectParams {
    name: String,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    description: Option<String>,
}

async fn do_insert(
    user: SqliteUser,
    params: &ProjectParams,
    state: &AppState,
) -> Result<Project, Error> {
    let mut project = Project::new(
        params.name.clone(),
        params.description.as_ref().cloned(),
        user.id,
    );
    project.insert(&state.db).await?;
    Ok(project)
}

async fn insert_project(
    user: SqliteUser,
    State(state): State<AppState>,
    Form(params): Form<ProjectParams>,
) -> Result<impl IntoResponse, Error> {
    if params.name.is_empty() {
        return Err(Error::RequiredParameterMissing("name".into()));
    }
    let project = do_insert(user, &params, &state).await?;
    debug!(project.id, "successfully inserted project");
    let projecturl = app_url(&format!("/project/{}", project.id));

    Ok((
        [("HX-Redirect", projecturl)],
        flash_message(
            state,
            FlashMessageKind::Success,
            format!(
                r#"Added new project {}: {} to the database"#,
                project.id, params.name
            ),
        ),
    )
        .into_response())
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
    Query(params): Query<ShowProjectQueryParams>,
    headers: HeaderMap,
    uri: OriginalUri,
) -> Result<impl IntoResponse, Error> {
    let mut project = Project::load_for_user(id, &user, &state.db).await?;
    let field = params.sort.as_ref().cloned().unwrap_or(SortField::Taxon);
    let sort = SortSpec::new(
        field.clone(),
        params.dir.as_ref().cloned().unwrap_or(SortOrder::Ascending),
    );
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

    let sort_options = vec![
        SortOption {
            code: SortField::Taxon,
            name: "Taxonomic Order".into(),
            selected: matches!(field, SortField::Taxon),
        },
        SortOption {
            code: SortField::SampleId,
            name: "Sample Id".into(),
            selected: matches!(field, SortField::SampleId),
        },
        SortOption {
            code: SortField::CollectionDate,
            name: "Date Collected".into(),
            selected: matches!(field, SortField::CollectionDate),
        },
        SortOption {
            code: SortField::Source,
            name: "Seed Source".into(),
            selected: matches!(field, SortField::Source),
        },
        SortOption {
            code: SortField::Quantity,
            name: "Quantity".into(),
            selected: matches!(field, SortField::Quantity),
        },
        SortOption {
            code: SortField::Activity,
            name: "Latest Activity".into(),
            selected: matches!(field, SortField::Activity),
        },
    ];

    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 project => project,
                 query => params,
                 options => sort_options,
                 summary => paginator,
                 request_uri => uri.to_string(),
                 filteronly => headers.get("HX-Request").is_some()),
    )
    .into_response())
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
            FlashMessageKind::Success,
            "Successfully updated project".to_string(),
        ),
    )
        .into_response())
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
            FlashMessageKind::Success,
            format!("Deleted project '{id}'"),
        ),
    )
        .into_response())
}

async fn add_sample_prep(
    user: &SqliteUser,
    id: <Project as Loadable>::Id,
    state: &AppState,
) -> Result<(Project, Vec<Sample>), Error> {
    let project = Project::load(id, &state.db).await?;

    let ids_in_project = sqlx::query!(
        "SELECT PS.sampleid from sc_project_samples PS WHERE PS.projectid=?",
        id
    )
    .fetch_all(state.db.pool())
    .await?;
    let ids = ids_in_project.iter().map(|row| row.sampleid).collect();

    /* FIXME: make this more efficient by changeing it to filter by
     *  'WHERE NOT IN (SELECT ids from...)'
     * instead of
     * [ query ids first ], then
     *  'WHERE NOT IN (1, 2, 3, 4, 5...)'
     */
    let samples = Sample::load_all(
        Some(
            and()
                .push(sample::Filter::IdNotIn(ids))
                .push(sample::Filter::UserId(user.id))
                .build(),
        ),
        None,
        None,
        &state.db,
    )
    .await?;
    Ok((project, samples))
}

async fn show_add_sample(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path(id): Path<<Project as Loadable>::Id>,
) -> Result<impl IntoResponse, Error> {
    let (project, samples) = add_sample_prep(&user, id, &state).await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 project => project,
                 samples => samples),
    )
    .into_response())
}

async fn add_sample(
    user: SqliteUser,
    State(state): State<AppState>,
    Path(id): Path<<Project as Loadable>::Id>,
    Form(params): Form<Vec<(String, String)>>,
) -> Result<impl IntoResponse, Error> {
    let mut messages = Vec::new();
    let toadd: HashSet<<Sample as Loadable>::Id> = params
        .iter()
        .filter_map(|(name, value)| match name.as_str() {
            "sample" => value.parse::<<Sample as Loadable>::Id>().ok(),
            _ => None,
        })
        .collect();
    let mut project = Project::load_for_user(id, &user, &state.db).await?;
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
            Err(libseed::Error::DatabaseError(sqlx::Error::Database(e)))
                if e.is_unique_violation() =>
            {
                messages.push(FlashMessage {
                    kind: FlashMessageKind::Warning,
                    msg: format!(
                        "Sample {} is already a member of this project",
                        format_id_number(id, Some("S"), None),
                    ),
                })
            }
            Err(libseed::Error::Unauthorized(message)) => tracing::error!(
                "Tried to add sample {id} to project {} without permission: {message}",
                project.id()
            ),
            Err(e) => {
                messages.push(FlashMessage {
                    kind: FlashMessageKind::Error,
                    msg: format!(
                        "Failed to add sample {}: Database error",
                        format_id_number(id, Some("S"), None),
                    ),
                });
                tracing::error!("Failed to add a sample to the project: {e}");
            }
        }
    }

    if n_inserted < toadd.len() {
        warn!("Some samples dropped, possibly because they were not owned by user {user:?}");
        let n_dropped = toadd.len() - n_inserted;
        messages.push(FlashMessage {
                kind: FlashMessageKind::Warning,
                msg: format!("{n_dropped} samples could not be added to the project. The samples may not exist or you may not have permissions to add them to this project.",),
            });
    }
    if n_inserted > 0 {
        messages.insert(
            0,
            FlashMessage {
                kind: FlashMessageKind::Success,
                msg: format!("Added {n_inserted} samples to this project"),
            },
        );
    } else {
        messages.insert(
            0,
            FlashMessage {
                kind: FlashMessageKind::Error,
                msg: "No samples were added to this project".to_string(),
            },
        );
    }

    Ok(flash_messages(state, &messages).into_response())
}
