use crate::{
    auth::SqliteUser,
    error::{self, Error},
    state::AppState,
    util::app_url,
    util::format_id_number,
    Message, MessageType, TemplateKey,
};
use anyhow::anyhow;
use axum::{
    extract::{rejection::QueryRejection, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::get,
    Form, Router,
};
use axum_template::RenderHtml;
use libseed::{
    empty_string_as_none,
    loadable::{ExternalRef, Loadable},
    project::{
        self,
        allocation::{self, taxon_name_like, SortField},
        Project,
    },
    query::{Cmp, CompoundFilter, Op, SortOrder, SortSpec},
    sample::{self, Sample},
};
use minijinja::context;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqliteQueryResult;
use std::{collections::HashSet, sync::Arc};
use tracing::{debug, trace, warn};

use super::{error_alert_response, SortOption};

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/new", get(show_new_project).post(insert_project))
        .route("/list", get(list_projects))
        .route(
            "/:id",
            get(show_project).put(modify_project).delete(delete_project),
        )
        .route("/:id/edit", get(show_project))
        .route("/:id/add", get(show_add_sample).post(add_sample))
        .nest("/:id/sample/", super::allocation::router())
}

#[derive(Debug, Deserialize, Serialize)]
struct ProjectListParams {
    #[serde(deserialize_with = "empty_string_as_none")]
    filter: Option<String>,
}

async fn list_projects(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    params: Option<Query<ProjectListParams>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, error::Error> {
    trace!(?params, "Listing projects");
    let mut fbuilder = CompoundFilter::builder(Op::And).push(project::Filter::User(user.id));
    let namefilter = params.and_then(|Query(p)| p.filter).map(|filterstring| {
        debug!(?filterstring, "Got project filter");
        CompoundFilter::builder(Op::Or)
            .push(project::Filter::Name(Cmp::Like, filterstring.clone()))
            .push(project::Filter::Description(
                Cmp::Like,
                filterstring.clone(),
            ))
            .build()
    });
    if let Some(namefilter) = namefilter {
        fbuilder = fbuilder.push(namefilter);
    }
    let projects = Project::load_all(Some(fbuilder.build()), &state.db).await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 projects => projects,
                 filteronly => headers.get("HX-Request").is_some()),
    )
    .into_response())
}

async fn show_new_project(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
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
) -> Result<SqliteQueryResult, error::Error> {
    let mut project = Project::new(
        params.name.clone(),
        params.description.as_ref().cloned(),
        user.id,
    );
    project.insert(&state.db).await.map_err(|e| e.into())
}

async fn insert_project(
    user: SqliteUser,
    State(state): State<AppState>,
    Form(params): Form<ProjectParams>,
) -> Result<impl IntoResponse, error::Error> {
    if params.name.is_empty() {
        return Ok(error_alert_response(
            &state,
            StatusCode::UNPROCESSABLE_ENTITY,
            "No name specified".to_string(),
        )
        .into_response());
    }
    match do_insert(user, &params, &state).await {
        Err(e) => {
            warn!("Failed to insert project: {e:?}");
            Ok(error_alert_response(
                &state,
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to save project".to_string(),
            )
            .into_response())
        }
        Ok(result) => {
            let id = result.last_insert_rowid();
            debug!(id, "successfully inserted project");
            let projecturl = app_url(&format!("/project/{}", id));

            Ok((
                [("HX-Redirect", projecturl)],
                RenderHtml(
                    "_ALERT.html",
                    state.tmpl.clone(),
                    context!( message =>
                    Message {
                        r#type: MessageType::Success,
                        msg: format!(
                            r#"Added new project {}: {} to the database"#,
                            id, params.name
                            )
                    },
                    ),
                ),
            )
                .into_response())
        }
    }
}

#[derive(Deserialize, Serialize)]
struct ShowProjectQueryParams {
    sort: Option<SortField>,
    dir: Option<SortOrder>,
    filter: Option<String>,
    _limit: Option<i32>,
    _offset: Option<i32>,
}

async fn show_project(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    Path(id): Path<i64>,
    State(state): State<AppState>,
    query: Result<Query<ShowProjectQueryParams>, QueryRejection>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, Error> {
    let Query(params) = query.map_err(Error::UnprocessableEntityQueryRejection)?;
    let fb = CompoundFilter::builder(Op::And)
        .push(project::Filter::Id(id))
        .push(project::Filter::User(user.id));

    let mut projects = Project::load_all(Some(fb.build()), &state.db).await?;
    let Some(mut project) = projects.pop() else {
        return Err(Error::NotFound("That project does not exist".to_string()));
    };

    let field = params.sort.as_ref().cloned().unwrap_or(SortField::Taxon);
    let sort = SortSpec::new(
        field.clone(),
        params.dir.as_ref().cloned().unwrap_or(SortOrder::Ascending),
    );
    let sample_filter = match params.filter {
        Some(ref fragment) if !fragment.trim().is_empty() => Some(
            CompoundFilter::builder(Op::Or)
                .push(taxon_name_like(fragment.clone()))
                .push(allocation::Filter::SourceName(Cmp::Like, fragment.clone()))
                .push(allocation::Filter::Notes(Cmp::Like, fragment.clone()))
                .build(),
        ),
        _ => None,
    };
    project
        .load_samples(sample_filter, Some(sort.into()), &state.db)
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
                 filteronly => headers.get("HX-Request").is_some()),
    )
    .into_response())
}

async fn do_update(
    id: i64,
    params: &ProjectParams,
    state: &AppState,
) -> Result<SqliteQueryResult, error::Error> {
    if params.name.is_empty() {
        return Err(anyhow!("No name specified").into());
    }
    let mut project = Project::load(id, &state.db).await?;
    project.name.clone_from(&params.name);
    project.description.clone_from(&params.description);
    project.update(&state.db).await.map_err(|e| e.into())
}

async fn modify_project(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    Path(id): Path<i64>,
    State(state): State<AppState>,
    Form(params): Form<ProjectParams>,
) -> Result<impl IntoResponse, error::Error> {
    let fb = CompoundFilter::builder(Op::And)
        .push(project::Filter::Id(id))
        .push(project::Filter::User(user.id));
    let projects = Project::load_all(Some(fb.build()), &state.db).await?;
    if projects.is_empty() {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let (request, message, headers) = match do_update(id, &params, &state).await {
        Err(e) => (
            Some(&params),
            Message {
                r#type: MessageType::Error,
                msg: e.to_string(),
            },
            None,
        ),
        Ok(_) => (
            None,
            Message {
                r#type: MessageType::Success,
                msg: "Successfully updated project".to_string(),
            },
            Some([("HX-Redirect", app_url(&format!("/project/{id}")))]),
        ),
    };
    let project = Project::load(id, &state.db).await?;
    Ok((
        headers,
        RenderHtml(
            key,
            state.tmpl.clone(),
            context!(project => project,
             message => message,
             request => request,
            ),
        ),
    )
        .into_response())
}

async fn delete_project(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let mut project = Project::load(id, &state.db)
        .await
        .map_err(|_| Error::NotFound("That project does not exist".to_string()))?;
    if project.userid != user.id {
        warn!(
            user.id,
            ?project,
            "User tried to delete project they don't own"
        );
        return Err(Error::Unauthorized(
            "No permission to delete this project".to_string(),
        ));
    }

    let errmsg = match project.delete(&state.db).await {
        Err(e) => {
            warn!(?e, "Failed to delete project");
            e.to_string()
        }
        Ok(res) if (res.rows_affected() == 0) => "No project found".to_string(),
        Ok(_) => {
            debug!(id, "Successfully deleted project");
            return Ok((
                [("HX-Redirect", app_url("/project/list"))],
                RenderHtml(key, state.tmpl.clone(), context!(deleted => true, id => id)),
            )
                .into_response());
        }
    };
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(
        project => project,
        message => Message {
            r#type: MessageType::Error,
            msg: format!("Failed to delete project: {errmsg}")
        },
        ),
    )
    .into_response())
}

async fn add_sample_prep(
    user: &SqliteUser,
    id: i64,
    state: &AppState,
) -> Result<(Project, Vec<Sample>), error::Error> {
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
    let samples = Sample::load_all_user(
        user.id,
        Some(Arc::new(sample::Filter::IdNotIn(ids))),
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
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, error::Error> {
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
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(params): Form<Vec<(String, String)>>,
) -> Result<impl IntoResponse, error::Error> {
    let mut messages = Vec::new();
    let toadd: HashSet<i64> = params
        .iter()
        .filter_map(|(name, value)| match name.as_str() {
            "sample" => value.parse::<i64>().ok(),
            _ => None,
        })
        .collect();
    let project = Project::load(id, &state.db).await?;
    if project.userid != user.id {
        // FIXME: maybe display a proper error message?
        return Ok(StatusCode::UNAUTHORIZED.into_response());
    }
    let mut fb = CompoundFilter::builder(Op::Or);
    for id in &toadd {
        fb = fb.push(sample::Filter::Id(Cmp::Equal, *id));
    }
    let valid_samples = Sample::load_all_user(user.id, Some(fb.build()), None, &state.db).await?;

    let valid_ids = valid_samples
        .iter()
        .map(|s| s.id())
        .collect::<HashSet<i64>>();
    let invalid = &toadd - &valid_ids;
    if !invalid.is_empty() {
        warn!("Some samples dropped, possibly because they were not owned by user {user:?}");
        let strval = invalid
            .iter()
            .map(|id| format_id_number(*id, Some("S"), None))
            .collect::<Vec<String>>()
            .join(", ");
        messages.push(Message {
            r#type: MessageType::Warning,
            msg: format!("Some samples could not be added to the project. The following samples may not exist or you may not have permissions to add them top this project: {strval}.",),
        })
    }

    let mut project = Project::load(id, &state.db).await?;
    let mut n_inserted = 0;
    for sample in valid_samples {
        let id = sample.id;
        match project
            .allocate_sample(ExternalRef::Object(sample), &state.db)
            .await
        {
            Ok(res) => n_inserted += res.rows_affected(),
            Err(libseed::Error::DatabaseError(sqlx::Error::Database(e)))
                if e.is_unique_violation() =>
            {
                messages.push(Message {
                    r#type: MessageType::Warning,
                    msg: format!(
                        "Sample {} is already a member of this project",
                        format_id_number(id, Some("S"), None),
                    ),
                })
            }
            Err(e) => {
                messages.push(Message {
                    r#type: MessageType::Error,
                    msg: format!(
                        "Failed to add sample {}: Database error",
                        format_id_number(id, Some("S"), None),
                    ),
                });
                tracing::error!("Failed to add a sample to the project: {e}");
            }
        }
    }

    if n_inserted > 0 {
        messages.insert(
            0,
            Message {
                r#type: MessageType::Success,
                msg: format!("Added {n_inserted} samples to this project"),
            },
        );
    } else {
        messages.insert(
            0,
            Message {
                r#type: MessageType::Error,
                msg: "No samples were added to this project".to_string(),
            },
        );
    }

    let (project, samples) = add_sample_prep(&user, id, &state).await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(project => project,
                 messages => messages,
                 samples => samples),
    )
    .into_response())
}
