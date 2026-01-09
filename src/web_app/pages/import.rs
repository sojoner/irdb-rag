use leptos::*;
use leptos::prelude::*;
use uuid::Uuid;
use crate::domain::dtos::{ImportJobResponse, ImportItemResponse};

#[server]
pub async fn create_import_job(
    source_type: String,
    source_path: String,
) -> Result<ImportJobResponse, ServerFnError> {
    use crate::api::state::AppState;
    use crate::services::import::{ImportJobRunner, ImportConfig, ImportItemManager, discover_files};
    use crate::infra::db;

    let state = use_context::<AppState>()
        .ok_or_else(|| ServerFnError::new("AppState not found in context"))?;

    let config = ImportConfig::from_env();
    let runner = ImportJobRunner::new(config);

    // Create the job
    let job_id = runner.create_job(
        &state.pool,
        &source_type,
        Some(&source_path),
    ).await.map_err(|e| ServerFnError::new(e.to_string()))?;

    // Discover files based on source type
    let files = if source_type == "folder" {
        discover_files(&source_path)
            .map_err(|e| ServerFnError::new(format!("Failed to discover files: {}", e)))?
    } else if source_type == "url" {
        // For URLs, split by newline and trim
        source_path.lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .map(std::path::PathBuf::from)
            .collect()
    } else {
        vec![]
    };

    // Create import items for discovered files
    let item_manager = ImportItemManager;
    let file_paths: Vec<&str> = files.iter()
        .filter_map(|p| p.to_str())
        .collect();

    if !file_paths.is_empty() {
        item_manager.create_items(&state.pool, job_id, file_paths)
            .await
            .map_err(|e| ServerFnError::new(format!("Failed to create import items: {}", e)))?;
    }

    // Update total_items count
    let total_items = files.len() as i32;
    runner.update_job_progress(&state.pool, job_id, total_items, 0, 0, 0)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    // Enqueue job for immediate processing
    state.import_job_queue.send(job_id).await
        .map_err(|e| ServerFnError::new(format!("Failed to enqueue import job: {}", e)))?;
    tracing::info!("Enqueued import job {} for processing", job_id);

    // Fetch the updated job
    let job = db::get_import_job(&state.pool, job_id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    Ok(ImportJobResponse {
        id: job.id,
        status: job.status,
        source_type: job.source_type,
        source_path: job.source_path,
        total_items: job.total_items,
        processed_items: job.processed_items,
        failed_items: job.failed_items,
        skipped_items: job.skipped_items,
        created_at: job.created_at.to_rfc3339(),
        started_at: job.started_at.map(|t| t.to_rfc3339()),
        completed_at: job.completed_at.map(|t| t.to_rfc3339()),
        error_message: job.error_message,
    })
}

#[server]
pub async fn list_import_jobs(
    limit: i64,
    offset: i64,
) -> Result<(Vec<ImportJobResponse>, i64), ServerFnError> {
    use crate::api::state::AppState;
    use crate::infra::db;

    let state = use_context::<AppState>()
        .ok_or_else(|| ServerFnError::new("AppState not found in context"))?;

    let (jobs, total) = db::list_import_jobs(&state.pool, limit as i32, offset as i32)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    let job_responses: Vec<ImportJobResponse> = jobs.into_iter().map(|job| {
        ImportJobResponse {
            id: job.id,
            status: job.status,
            source_type: job.source_type,
            source_path: job.source_path,
            total_items: job.total_items,
            processed_items: job.processed_items,
            failed_items: job.failed_items,
            skipped_items: job.skipped_items,
            created_at: job.created_at.to_rfc3339(),
            started_at: job.started_at.map(|t| t.to_rfc3339()),
            completed_at: job.completed_at.map(|t| t.to_rfc3339()),
            error_message: job.error_message,
        }
    }).collect();

    Ok((job_responses, total))
}

#[server]
pub async fn get_import_job_details(
    job_id: Uuid,
) -> Result<ImportJobResponse, ServerFnError> {
    use crate::api::state::AppState;
    use crate::infra::db;

    let state = use_context::<AppState>()
        .ok_or_else(|| ServerFnError::new("AppState not found in context"))?;

    let job = db::get_import_job(&state.pool, job_id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    Ok(ImportJobResponse {
        id: job.id,
        status: job.status,
        source_type: job.source_type,
        source_path: job.source_path,
        total_items: job.total_items,
        processed_items: job.processed_items,
        failed_items: job.failed_items,
        skipped_items: job.skipped_items,
        created_at: job.created_at.to_rfc3339(),
        started_at: job.started_at.map(|t| t.to_rfc3339()),
        completed_at: job.completed_at.map(|t| t.to_rfc3339()),
        error_message: job.error_message,
    })
}

#[server]
pub async fn get_import_job_items(
    job_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<ImportItemResponse>, i64), ServerFnError> {
    use crate::api::state::AppState;
    use crate::infra::db;

    let state = use_context::<AppState>()
        .ok_or_else(|| ServerFnError::new("AppState not found in context"))?;

    let (items, total) = db::get_import_items(&state.pool, job_id, limit as i32, offset as i32)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    let item_responses: Vec<ImportItemResponse> = items.into_iter().map(|item| {
        ImportItemResponse {
            id: item.id,
            job_id: item.job_id,
            source_path: item.source_path,
            status: item.status,
            retry_count: item.retry_count,
            error_message: item.error_message,
            error_type: item.error_type,
            document_id: item.document_id,
        }
    }).collect();

    Ok((item_responses, total))
}

#[server]
pub async fn resume_import_job(
    job_id: Uuid,
) -> Result<ImportJobResponse, ServerFnError> {
    use crate::api::state::AppState;
    use crate::infra::db;
    use crate::services::import::{ImportJobRunner, ImportConfig};

    let state = use_context::<AppState>()
        .ok_or_else(|| ServerFnError::new("AppState not found in context"))?;

    let config = ImportConfig::from_env();
    let runner = ImportJobRunner::new(config);

    // Update job status to running
    runner.update_job_status(&state.pool, job_id, "running")
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    let job = db::get_import_job(&state.pool, job_id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    Ok(ImportJobResponse {
        id: job.id,
        status: job.status,
        source_type: job.source_type,
        source_path: job.source_path,
        total_items: job.total_items,
        processed_items: job.processed_items,
        failed_items: job.failed_items,
        skipped_items: job.skipped_items,
        created_at: job.created_at.to_rfc3339(),
        started_at: job.started_at.map(|t| t.to_rfc3339()),
        completed_at: job.completed_at.map(|t| t.to_rfc3339()),
        error_message: job.error_message,
    })
}

#[server]
pub async fn delete_import_job(
    job_id: Uuid,
    delete_documents: bool,
) -> Result<(), ServerFnError> {
    use crate::api::state::AppState;
    use crate::services::import::{ImportJobRunner, ImportConfig};

    let state = use_context::<AppState>()
        .ok_or_else(|| ServerFnError::new("AppState not found in context"))?;

    let config = ImportConfig::from_env();
    let runner = ImportJobRunner::new(config);

    runner.delete_job(&state.pool, job_id, delete_documents)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    Ok(())
}

#[server]
pub async fn delete_import_item(
    item_id: Uuid,
    delete_document: bool,
) -> Result<u64, ServerFnError> {
    use crate::api::state::AppState;
    use crate::infra::db;

    let state = use_context::<AppState>()
        .ok_or_else(|| ServerFnError::new("AppState not found"))?;

    // If delete_document is true, first get the document ID and delete it
    if delete_document {
        if let Some(item) = db::get_import_item(&state.pool, item_id)
            .await
            .ok()
            .flatten()
        {
            if let Some(doc_id) = item.document_id {
                db::delete_document(&state.pool, doc_id)
                    .await
                    .map_err(|e| ServerFnError::new(e.to_string()))?;
            }
        }
    }

    // Delete the import item
    let result = db::delete_import_item(&state.pool, item_id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    Ok(result)
}

#[component]
pub fn ImportPage() -> impl IntoView {
    // State
    let (source_type, set_source_type) = signal("folder".to_string());
    let (source_path, set_source_path) = signal(String::new());
    let (show_create_modal, set_show_create_modal) = signal(false);
    let (selected_job_id, set_selected_job_id) = signal(None::<Uuid>);
    let (refresh_trigger, set_refresh_trigger) = signal(0);
    let (selected_jobs, set_selected_jobs) = signal(std::collections::HashSet::<Uuid>::new());
    let (show_bulk_delete_modal, set_show_bulk_delete_modal) = signal(false);

    // Server Actions
    let create_action = ServerAction::<CreateImportJob>::new();
    let list_action = ServerAction::<ListImportJobs>::new();
    let details_action = ServerAction::<GetImportJobDetails>::new();
    let items_action = ServerAction::<GetImportJobItems>::new();
    let resume_action = ServerAction::<ResumeImportJob>::new();
    let delete_action = ServerAction::<DeleteImportJob>::new();
    let delete_item_action = ServerAction::<DeleteImportItem>::new();

    // Load jobs on mount and refresh
    Effect::new(move |_| {
        let _ = refresh_trigger.get();
        list_action.dispatch(ListImportJobs {
            limit: 50,
            offset: 0,
        });
    });

    // Handle job creation success
    Effect::new(move |_| {
        if let Some(Ok(_)) = create_action.value().get() {
            set_show_create_modal.set(false);
            set_source_path.set(String::new());
            set_refresh_trigger.update(|n| *n += 1);
        }
    });

    // Handle resume success
    Effect::new(move |_| {
        if let Some(Ok(_)) = resume_action.value().get() {
            set_refresh_trigger.update(|n| *n += 1);
        }
    });

    // Handle delete success
    Effect::new(move |_| {
        if let Some(Ok(_)) = delete_action.value().get() {
            set_selected_job_id.set(None);
            set_refresh_trigger.update(|n| *n += 1);
        }
    });

    // Handle delete item success
    Effect::new(move |_| {
        if let Some(Ok(_)) = delete_item_action.value().get() {
            if let Some(job_id) = selected_job_id.get() {
                items_action.dispatch(GetImportJobItems {
                    job_id,
                    limit: 100,
                    offset: 0,
                });
            }
        }
    });

    // Load job details when selected
    Effect::new(move |_| {
        if let Some(job_id) = selected_job_id.get() {
            details_action.dispatch(GetImportJobDetails { job_id });
            items_action.dispatch(GetImportJobItems {
                job_id,
                limit: 100,
                offset: 0,
            });
        }
    });

    let jobs_list = move || {
        list_action.value().get()
            .and_then(|res| res.ok())
            .map(|(jobs, _total)| jobs)
            .unwrap_or_default()
    };

    let job_details = move || {
        details_action.value().get()
            .and_then(|res| res.ok())
    };

    let job_items = move || {
        items_action.value().get()
            .and_then(|res| res.ok())
            .map(|(items, _total)| items)
            .unwrap_or_default()
    };

    let handle_create = move |_| {
        let path = source_path.get();
        if path.trim().is_empty() {
            return;
        }

        create_action.dispatch(CreateImportJob {
            source_type: source_type.get(),
            source_path: path,
        });
    };

    let handle_resume = move |job_id: Uuid| {
        resume_action.dispatch(ResumeImportJob { job_id });
    };

    let handle_refresh = move |_| {
        set_refresh_trigger.update(|n| *n += 1);
    };

    let toggle_job_selection = move |job_id: Uuid| {
        set_selected_jobs.update(|jobs| {
            if jobs.contains(&job_id) {
                jobs.remove(&job_id);
            } else {
                jobs.insert(job_id);
            }
        });
    };

    let select_all_jobs = move |_| {
        let all_ids: Vec<Uuid> = jobs_list().iter().map(|j| j.id).collect();
        set_selected_jobs.set(all_ids.into_iter().collect());
    };

    let deselect_all_jobs = move |_| {
        set_selected_jobs.set(std::collections::HashSet::new());
    };

    let handle_bulk_delete = move |delete_documents: bool| {
        let jobs_to_delete: Vec<Uuid> = selected_jobs.get().into_iter().collect();
        for job_id in jobs_to_delete {
            delete_action.dispatch(DeleteImportJob { job_id, delete_documents });
        }
        set_selected_jobs.set(std::collections::HashSet::new());
        set_show_bulk_delete_modal.set(false);
    };

    view! {
        <div class="flex flex-col h-screen bg-gray-50">
            // HEADER
            <header class="bg-white shadow-sm border-b border-gray-200">
                <div class="px-6 py-4 flex justify-between items-center">
                    <div class="flex items-center gap-4">
                        <a href="/" class="text-gray-600 hover:text-gray-900 transition-colors">
                            <svg class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 19l-7-7m0 0l7-7m-7 7h18" />
                            </svg>
                        </a>
                        <h1 class="text-2xl font-bold text-gray-900">"Import Manager"</h1>
                    </div>
                    <div class="flex gap-2 items-center">
                        <Show when=move || !selected_jobs.get().is_empty()>
                            <span class="text-sm text-gray-600 px-2">
                                {move || format!("{} selected", selected_jobs.get().len())}
                            </span>
                            <button
                                on:click=deselect_all_jobs
                                class="px-3 py-2 text-xs font-medium text-gray-600 bg-white border border-gray-300 rounded-md hover:bg-gray-50 transition-colors"
                            >
                                "Clear Selection"
                            </button>
                            <button
                                on:click=move |_| set_show_bulk_delete_modal.set(true)
                                class="px-3 py-2 text-xs font-medium text-white bg-red-600 rounded-md hover:bg-red-700 transition-colors flex items-center gap-1"
                            >
                                <svg class="h-3 w-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                                </svg>
                                "Delete Selected"
                            </button>
                        </Show>
                        <button
                            on:click=handle_refresh
                            class="px-3 py-2 text-sm font-medium text-gray-700 bg-white border border-gray-300 rounded-md hover:bg-gray-50 transition-colors flex items-center gap-2"
                        >
                            <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
                            </svg>
                            "Refresh"
                        </button>
                        <button
                            on:click=move |_| set_show_create_modal.set(true)
                            class="px-4 py-2 text-sm font-medium text-white bg-blue-600 rounded-md hover:bg-blue-700 transition-colors flex items-center gap-2"
                        >
                            <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
                            </svg>
                            "New Import Job"
                        </button>
                    </div>
                </div>
            </header>

            // MAIN CONTENT
            <div class="flex-1 overflow-hidden p-6">
                <div class="h-full flex gap-6">
                    // LEFT: Jobs List (40%)
                    <div class="w-2/5 bg-white rounded-lg shadow-sm border border-gray-200 flex flex-col overflow-hidden">
                        <div class="px-4 py-3 border-b border-gray-200 bg-gray-50 flex justify-between items-center">
                            <h2 class="text-sm font-bold text-gray-700">"Import Jobs"</h2>
                            <div class="flex gap-2">
                                <button
                                    on:click=select_all_jobs
                                    class="text-xs text-blue-600 hover:text-blue-700 font-medium"
                                >
                                    "Select All"
                                </button>
                            </div>
                        </div>
                        <div class="flex-1 overflow-y-auto p-4 space-y-2">
                            <Show
                                when=move || list_action.pending().get()
                                fallback=move || view! {
                                    <For
                                        each=jobs_list
                                        key=|job| job.id
                                        children=move |job| {
                                            let job_id = job.id;
                                            let is_selected = move || selected_job_id.get() == Some(job_id);
                                            let is_checked = move || selected_jobs.get().contains(&job_id);
                                            let status = job.status.clone();
                                            let source_type = job.source_type.clone();
                                            let source_path_opt = job.source_path.clone();
                                            let total = job.total_items;
                                            let processed = job.processed_items;
                                            let failed = job.failed_items;
                                            let skipped = job.skipped_items;

                                            view! {
                                                <div
                                                    class=move || format!(
                                                        "p-4 rounded-lg border-2 transition-all {}",
                                                        if is_selected() {
                                                            "border-blue-500 bg-blue-50"
                                                        } else {
                                                            "border-gray-200 hover:border-gray-300 bg-white"
                                                        }
                                                    )
                                                >
                                                    <div class="flex items-start justify-between gap-3">
                                                        <div class="flex items-start gap-3 flex-1 min-w-0">
                                                            <input
                                                                type="checkbox"
                                                                checked=is_checked
                                                                on:click=move |e| {
                                                                    e.stop_propagation();
                                                                    toggle_job_selection(job_id);
                                                                }
                                                                class="mt-1 h-4 w-4 text-blue-600 border-gray-300 rounded focus:ring-blue-500 cursor-pointer"
                                                            />
                                                            <div
                                                                on:click=move |_| set_selected_job_id.set(Some(job_id))
                                                                class="flex-1 min-w-0 cursor-pointer"
                                                            >
                                                                <div class="flex items-center gap-2 mb-1">
                                                                    <StatusBadge status=status.clone() />
                                                                    <span class="text-xs text-gray-500">
                                                                        {source_type.clone()}
                                                                    </span>
                                                                </div>
                                                                <p class="text-sm font-medium text-gray-900 truncate mb-1">
                                                                    {source_path_opt.clone().unwrap_or_default()}
                                                                </p>
                                                                <div class="flex items-center gap-3 text-xs text-gray-600">
                                                                    <span>{format!("Total: {}", total)}</span>
                                                                    <span class="text-green-600">{format!("✓ {}", processed - failed - skipped)}</span>
                                                                    <span class="text-red-600">{format!("✗ {}", failed)}</span>
                                                                    <span class="text-yellow-600">{format!("⊘ {}", skipped)}</span>
                                                                </div>
                                                            </div>
                                                        </div>
                                                        <button
                                                            on:click=move |e| {
                                                                e.stop_propagation();
                                                                delete_action.dispatch(DeleteImportJob { job_id, delete_documents: false });
                                                            }
                                                            class="text-gray-400 hover:text-red-600 transition-colors p-1 flex-shrink-0"
                                                            title="Delete job"
                                                        >
                                                            <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                                                                      d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                                                            </svg>
                                                        </button>
                                                    </div>

                                                    {
                                                        let should_show = total > 0;
                                                        view! {
                                                            <Show when=move || should_show>
                                                        <div class="mt-2">
                                                            <div class="w-full bg-gray-200 rounded-full h-1.5">
                                                                <div
                                                                    class="bg-blue-600 h-1.5 rounded-full transition-all"
                                                                    style=move || {
                                                                        let percent = if total > 0 {
                                                                            (processed as f64 / total as f64 * 100.0) as i32
                                                                        } else {
                                                                            0
                                                                        };
                                                                        format!("width: {}%", percent)
                                                                    }
                                                                />
                                                            </div>
                                                        </div>
                                                    </Show>
                                                        }
                                                    }
                                                </div>
                                            }
                                        }
                                    />

                                    <Show when=move || jobs_list().is_empty()>
                                        <div class="text-center py-12 text-gray-500">
                                            <svg class="mx-auto h-12 w-12 text-gray-400 mb-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M20 13V6a2 2 0 00-2-2H6a2 2 0 00-2 2v7m16 0v5a2 2 0 01-2 2H6a2 2 0 01-2-2v-5m16 0h-2.586a1 1 0 00-.707.293l-2.414 2.414a1 1 0 01-.707.293h-3.172a1 1 0 01-.707-.293l-2.414-2.414A1 1 0 006.586 13H4" />
                                            </svg>
                                            <p class="text-sm">"No import jobs yet"</p>
                                            <p class="text-xs mt-1">"Create your first import job to get started"</p>
                                        </div>
                                    </Show>
                                }
                            >
                                <div class="text-center py-12">
                                    <div class="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-600 mx-auto"></div>
                                    <p class="text-sm text-gray-500 mt-2">"Loading jobs..."</p>
                                </div>
                            </Show>
                        </div>
                    </div>

                    // RIGHT: Job Details (60%)
                    <div class="flex-1 bg-white rounded-lg shadow-sm border border-gray-200 flex flex-col overflow-hidden">
                        <Show
                            when=move || selected_job_id.get().is_some()
                            fallback=|| view! {
                                <div class="flex-1 flex items-center justify-center text-gray-500">
                                    <div class="text-center">
                                        <svg class="mx-auto h-12 w-12 text-gray-400 mb-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
                                        </svg>
                                        <p class="text-sm">"Select a job to view details"</p>
                                    </div>
                                </div>
                            }
                        >
                            {move || {
                                job_details().map(|job| {
                                    let job_id = job.id;
                                    let can_resume = (job.status == "failed" || job.status == "completed") && job.failed_items > 0;
                                    let status = job.status.clone();
                                    let source_type = job.source_type.clone();
                                    let source_path = job.source_path.clone().unwrap_or_default();
                                    let total = job.total_items;
                                    let processed = job.processed_items;
                                    let failed = job.failed_items;
                                    let skipped = job.skipped_items;

                                    view! {
                                        <div class="flex flex-col h-full">
                                            // Job Header
                                            <div class="px-4 py-3 border-b border-gray-200 bg-gray-50">
                                                <div class="flex items-center justify-between mb-2">
                                                    <h2 class="text-sm font-bold text-gray-700">"Job Details"</h2>
                                                    <div class="flex gap-2">
                                                        <Show when=move || can_resume>
                                                            <button
                                                                on:click=move |_| handle_resume(job_id)
                                                                class="px-3 py-1 text-xs font-medium text-white bg-green-600 rounded hover:bg-green-700 transition-colors"
                                                            >
                                                                "Resume Failed Items"
                                                            </button>
                                                        </Show>
                                                        <button
                                                            on:click=move |_| {
                                                                delete_action.dispatch(DeleteImportJob { job_id, delete_documents: false });
                                                            }
                                                            class="px-3 py-1 text-xs font-medium text-white bg-red-600 rounded hover:bg-red-700 transition-colors"
                                                        >
                                                            "Delete Job"
                                                        </button>
                                                    </div>
                                                </div>
                                                <div class="flex items-center gap-2 mb-2">
                                                    <StatusBadge status=status.clone() />
                                                    <span class="text-xs text-gray-500">{source_type.clone()}</span>
                                                </div>
                                                <p class="text-sm text-gray-900 mb-3 break-all">{source_path.clone()}</p>

                                                // Progress Stats
                                                <div class="grid grid-cols-4 gap-2 text-xs">
                                                    <div class="bg-white rounded p-2 border border-gray-200">
                                                        <div class="text-gray-600">"Total"</div>
                                                        <div class="text-lg font-bold text-gray-900">{total}</div>
                                                    </div>
                                                    <div class="bg-green-50 rounded p-2 border border-green-200">
                                                        <div class="text-green-700">"Completed"</div>
                                                        <div class="text-lg font-bold text-green-900">
                                                            {processed - failed - skipped}
                                                        </div>
                                                    </div>
                                                    <div class="bg-red-50 rounded p-2 border border-red-200">
                                                        <div class="text-red-700">"Failed"</div>
                                                        <div class="text-lg font-bold text-red-900">{failed}</div>
                                                    </div>
                                                    <div class="bg-yellow-50 rounded p-2 border border-yellow-200">
                                                        <div class="text-yellow-700">"Skipped"</div>
                                                        <div class="text-lg font-bold text-yellow-900">{skipped}</div>
                                                    </div>
                                                </div>

                                                {
                                                    let show_progress = total > 0;
                                                    view! {
                                                        <Show when=move || show_progress>
                                                    <div class="mt-3">
                                                        <div class="flex justify-between text-xs text-gray-600 mb-1">
                                                            <span>"Progress"</span>
                                                            <span>
                                                                {format!("{} / {} ({}%)",
                                                                    processed,
                                                                    total,
                                                                    (processed as f64 / total as f64 * 100.0) as i32
                                                                )}
                                                            </span>
                                                        </div>
                                                        <div class="w-full bg-gray-200 rounded-full h-2">
                                                            <div
                                                                class="bg-blue-600 h-2 rounded-full transition-all"
                                                                style=move || {
                                                                    let percent = if total > 0 {
                                                                        (processed as f64 / total as f64 * 100.0) as i32
                                                                    } else {
                                                                        0
                                                                    };
                                                                    format!("width: {}%", percent)
                                                                }
                                                            />
                                                        </div>
                                                    </div>
                                                </Show>
                                                    }
                                                }
                                            </div>

                                            // Items List
                                            <div class="flex-1 overflow-y-auto p-4">
                                                <h3 class="text-xs font-bold text-gray-700 mb-2 uppercase">"Import Items"</h3>
                                                <div class="space-y-2">
                                                    <For
                                                        each=job_items
                                                        key=|item| item.id
                                                        children=move |item| {
                                                            let status = item.status.clone();
                                                            let retry_count = item.retry_count;
                                                            let source_path = item.source_path.clone();
                                                            let error_msg = item.error_message.clone();

                                                            view! {
                                                                <div class="p-3 rounded-lg border border-gray-200 bg-gray-50 hover:bg-gray-100 transition-colors">
                                                                    <div class="flex items-start justify-between gap-2">
                                                                        <div class="flex-1 min-w-0">
                                                                            <div class="flex items-center gap-2 mb-1">
                                                                                <ItemStatusBadge status=status.clone() />
                                                                                {
                                                                                    let has_retries = retry_count > 0;
                                                                                    view! {
                                                                                        <Show when=move || has_retries>
                                                                                    <span class="text-xs text-gray-500">
                                                                                        {format!("Retry: {}", retry_count)}
                                                                                    </span>
                                                                                </Show>
                                                                                    }
                                                                                }
                                                                            </div>
                                                                            <p class="text-xs font-medium text-gray-900 truncate mb-1">
                                                                                {source_path.clone()}
                                                                            </p>
                                                                            {
                                                                                let has_error = error_msg.is_some();
                                                                                let err_msg = error_msg.clone();
                                                                                view! {
                                                                                    <Show when=move || has_error>
                                                                                        <p class="text-xs text-red-600 mt-1">
                                                                                            {err_msg.clone().unwrap_or_default()}
                                                                                        </p>
                                                                                    </Show>
                                                                                }
                                                                            }
                                                                        </div>
                                                                        <button
                                                                            on:click=move |_| {
                                                                                if web_sys::window()
                                                                                    .and_then(|w| w.confirm_with_message("Delete this import item? Also delete the imported document?").ok())
                                                                                    .unwrap_or(false)
                                                                                {
                                                                                    delete_item_action.dispatch(DeleteImportItem {
                                                                                        item_id: item.id,
                                                                                        delete_document: true
                                                                                    });
                                                                                }
                                                                            }
                                                                            class="text-gray-400 hover:text-red-600 transition-colors p-1 flex-shrink-0"
                                                                            title="Delete item"
                                                                        >
                                                                            <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                                                                                      d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                                                                            </svg>
                                                                        </button>
                                                                    </div>
                                                                </div>
                                                            }
                                                        }
                                                    />

                                                    <Show when=move || job_items().is_empty()>
                                                        <div class="text-center py-8 text-gray-500">
                                                            <p class="text-sm">"No items yet"</p>
                                                        </div>
                                                    </Show>
                                                </div>
                                            </div>
                                        </div>
                                    }
                                })
                            }}
                        </Show>
                    </div>
                </div>
            </div>

            // CREATE MODAL
            <Show when=move || show_create_modal.get()>
                <div class="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
                    <div class="bg-white rounded-lg shadow-xl max-w-lg w-full mx-4">
                        <div class="px-6 py-4 border-b border-gray-200 flex justify-between items-center">
                            <h3 class="text-lg font-bold text-gray-900">"Create Import Job"</h3>
                            <button
                                on:click=move |_| set_show_create_modal.set(false)
                                class="text-gray-400 hover:text-gray-600 transition-colors"
                            >
                                <svg class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                                </svg>
                            </button>
                        </div>

                        <div class="p-6 space-y-4">
                            // Source Type Selection
                            <div>
                                <label class="block text-sm font-medium text-gray-700 mb-2">"Source Type"</label>
                                <div class="grid grid-cols-2 gap-2">
                                    <button
                                        on:click=move |_| set_source_type.set("folder".to_string())
                                        class=move || format!(
                                            "px-4 py-3 rounded-lg border-2 transition-all text-left {}",
                                            if source_type.get() == "folder" {
                                                "border-blue-500 bg-blue-50"
                                            } else {
                                                "border-gray-200 hover:border-gray-300"
                                            }
                                        )
                                    >
                                        <div class="font-medium text-sm text-gray-900">"Folder"</div>
                                        <div class="text-xs text-gray-600">"Import from directory"</div>
                                    </button>
                                    <button
                                        on:click=move |_| set_source_type.set("url".to_string())
                                        class=move || format!(
                                            "px-4 py-3 rounded-lg border-2 transition-all text-left {}",
                                            if source_type.get() == "url" {
                                                "border-blue-500 bg-blue-50"
                                            } else {
                                                "border-gray-200 hover:border-gray-300"
                                            }
                                        )
                                    >
                                        <div class="font-medium text-sm text-gray-900">"URL"</div>
                                        <div class="text-xs text-gray-600">"Import from web"</div>
                                    </button>
                                </div>
                            </div>

                            // Source Path Input
                            <div>
                                <label class="block text-sm font-medium text-gray-700 mb-2">
                                    {move || if source_type.get() == "folder" { "Folder Path" } else { "URL" }}
                                </label>
                                <textarea
                                    prop:value=move || source_path.get()
                                    on:input=move |ev| set_source_path.set(event_target_value(&ev))
                                    placeholder=move || if source_type.get() == "folder" {
                                        "/path/to/documents"
                                    } else {
                                        "https://example.com/document.pdf\nhttps://example.com/another.pdf"
                                    }
                                    rows="4"
                                    class="w-full px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-blue-500 focus:border-blue-500 text-sm"
                                />
                                <p class="text-xs text-gray-500 mt-1">
                                    {move || if source_type.get() == "folder" {
                                        "Enter the absolute path to the folder containing documents"
                                    } else {
                                        "Enter one or more URLs (one per line)"
                                    }}
                                </p>
                            </div>

                            <Show when=move || create_action.value().get().and_then(|res| res.err()).is_some()>
                                <div class="bg-red-50 border border-red-200 rounded-lg p-3">
                                    <p class="text-sm text-red-700">
                                        {move || create_action.value().get().and_then(|res| res.err()).unwrap().to_string()}
                                    </p>
                                </div>
                            </Show>
                        </div>

                        <div class="px-6 py-4 border-t border-gray-200 flex justify-end gap-2">
                            <button
                                on:click=move |_| set_show_create_modal.set(false)
                                class="px-4 py-2 text-sm font-medium text-gray-700 bg-white border border-gray-300 rounded-md hover:bg-gray-50 transition-colors"
                            >
                                "Cancel"
                            </button>
                            <button
                                on:click=handle_create
                                disabled=move || source_path.get().trim().is_empty() || create_action.pending().get()
                                class="px-4 py-2 text-sm font-medium text-white bg-blue-600 rounded-md hover:bg-blue-700 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                            >
                                {move || if create_action.pending().get() { "Creating..." } else { "Create Job" }}
                            </button>
                        </div>
                    </div>
                </div>
            </Show>

            // BULK DELETE CONFIRMATION MODAL
            <Show when=move || show_bulk_delete_modal.get()>
                <div class="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
                    <div class="bg-white rounded-lg shadow-xl max-w-md w-full mx-4">
                        <div class="px-6 py-4 border-b border-gray-200">
                            <h3 class="text-lg font-bold text-gray-900">"Confirm Bulk Delete"</h3>
                        </div>

                        <div class="p-6 space-y-4">
                            <div class="flex items-start gap-3">
                                <svg class="h-6 w-6 text-red-600 flex-shrink-0 mt-0.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                                </svg>
                                <div>
                                    <p class="text-sm text-gray-700 font-medium mb-2">
                                        {move || format!("You are about to delete {} import job(s).", selected_jobs.get().len())}
                                    </p>
                                    <p class="text-sm text-gray-600">
                                        "This action cannot be undone. Do you also want to delete the documents that were imported by these jobs?"
                                    </p>
                                </div>
                            </div>

                            <div class="bg-yellow-50 border border-yellow-200 rounded-lg p-3">
                                <p class="text-xs text-yellow-800">
                                    <strong>"Warning:"</strong> " Deleting documents will permanently remove them and all associated data (chunks, embeddings, etc.)"
                                </p>
                            </div>
                        </div>

                        <div class="px-6 py-4 border-t border-gray-200 flex justify-end gap-2">
                            <button
                                on:click=move |_| set_show_bulk_delete_modal.set(false)
                                class="px-4 py-2 text-sm font-medium text-gray-700 bg-white border border-gray-300 rounded-md hover:bg-gray-50 transition-colors"
                            >
                                "Cancel"
                            </button>
                            <button
                                on:click=move |_| handle_bulk_delete(false)
                                class="px-4 py-2 text-sm font-medium text-white bg-orange-600 rounded-md hover:bg-orange-700 transition-colors"
                            >
                                "Delete Jobs Only"
                            </button>
                            <button
                                on:click=move |_| handle_bulk_delete(true)
                                class="px-4 py-2 text-sm font-medium text-white bg-red-600 rounded-md hover:bg-red-700 transition-colors"
                            >
                                "Delete Jobs & Documents"
                            </button>
                        </div>
                    </div>
                </div>
            </Show>

        </div>
    }
}

#[component]
fn StatusBadge(status: String) -> impl IntoView {
    let (color, text) = match status.as_str() {
        "pending" => ("bg-gray-100 text-gray-800", "Pending"),
        "running" => ("bg-blue-100 text-blue-800", "Running"),
        "completed" => ("bg-green-100 text-green-800", "Completed"),
        "failed" => ("bg-red-100 text-red-800", "Failed"),
        "cancelled" => ("bg-gray-100 text-gray-800", "Cancelled"),
        _ => ("bg-gray-100 text-gray-800", "Unknown"),
    };

    view! {
        <span class=format!("px-2 py-1 text-xs font-medium rounded-full {}", color)>
            {text}
        </span>
    }
}

#[component]
fn ItemStatusBadge(status: String) -> impl IntoView {
    let (color, text) = match status.as_str() {
        "pending" => ("bg-gray-100 text-gray-700", "⏳ Pending"),
        "processing" => ("bg-blue-100 text-blue-700", "⚙️ Processing"),
        "completed" => ("bg-green-100 text-green-700", "✓ Completed"),
        "failed" => ("bg-red-100 text-red-700", "✗ Failed"),
        "skipped" => ("bg-yellow-100 text-yellow-700", "⊘ Skipped"),
        _ => ("bg-gray-100 text-gray-700", "Unknown"),
    };

    view! {
        <span class=format!("px-2 py-0.5 text-xs font-medium rounded {}", color)>
            {text}
        </span>
    }
}
