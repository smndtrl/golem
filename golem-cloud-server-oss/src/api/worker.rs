use std::str::FromStr;
use std::sync::Arc;

use crate::api::ApiTags;
use golem_common::model::{CallingConvention, InvocationKey, TemplateId};
use poem_openapi::param::{Path, Query};
use poem_openapi::payload::Json;
use poem_openapi::*;
use tap::TapFallible;
use tonic::Status;

use crate::service::template::{TemplateError, TemplateService};
use crate::service::worker::WorkerService;
use golem_cloud_server_base::model::*;

#[derive(ApiResponse)]
pub enum WorkerError {
    #[oai(status = 400)]
    BadRequest(Json<ErrorsBody>),
    #[oai(status = 404)]
    NotFound(Json<ErrorBody>),
    #[oai(status = 409)]
    AlreadyExists(Json<ErrorBody>),
    #[oai(status = 500)]
    InternalError(Json<GolemErrorBody>),
}

type Result<T> = std::result::Result<T, WorkerError>;

impl From<tonic::transport::Error> for WorkerError {
    fn from(value: tonic::transport::Error) -> Self {
        WorkerError::InternalError(Json(GolemErrorBody {
            golem_error: GolemError::Unknown(GolemErrorUnknown {
                details: value.to_string(),
            }),
        }))
    }
}

impl From<Status> for WorkerError {
    fn from(value: Status) -> Self {
        WorkerError::InternalError(Json(GolemErrorBody {
            golem_error: GolemError::Unknown(GolemErrorUnknown {
                details: value.to_string(),
            }),
        }))
    }
}

impl From<String> for WorkerError {
    fn from(value: String) -> Self {
        WorkerError::InternalError(Json(GolemErrorBody {
            golem_error: GolemError::Unknown(GolemErrorUnknown { details: value }),
        }))
    }
}

impl From<crate::service::worker::WorkerError> for WorkerError {
    fn from(value: crate::service::worker::WorkerError) -> Self {
        use crate::service::worker::WorkerError as ServiceError;

        match value {
            ServiceError::Internal(error) => WorkerError::InternalError(Json(GolemErrorBody {
                golem_error: GolemError::Unknown(GolemErrorUnknown { details: error }),
            })),
            ServiceError::TypeCheckerError(error) => WorkerError::BadRequest(Json(ErrorsBody {
                errors: vec![format!("Type checker error: {error}")],
            })),
            ServiceError::VersionedTemplateIdNotFound(template_id) => {
                WorkerError::NotFound(Json(ErrorBody {
                    error: format!("Template not found: {template_id}"),
                }))
            }
            ServiceError::TemplateNotFound(template_id) => WorkerError::NotFound(Json(ErrorBody {
                error: format!("Template not found: {template_id}"),
            })),
            ServiceError::AccountIdNotFound(account_id) => WorkerError::NotFound(Json(ErrorBody {
                error: format!("Account not found: {account_id}"),
            })),
            ServiceError::WorkerNotFound(worker_id) => WorkerError::NotFound(Json(ErrorBody {
                error: format!("Worker not found: {worker_id}"),
            })),
            ServiceError::Golem(golem_error) => {
                WorkerError::InternalError(Json(GolemErrorBody { golem_error }))
            }
            ServiceError::DelegatedTemplateServiceError(error) => error.into(),
        }
    }
}

impl From<TemplateError> for WorkerError {
    fn from(value: TemplateError) -> Self {
        match value {
            TemplateError::Internal(error) => WorkerError::InternalError(Json(GolemErrorBody {
                golem_error: GolemError::Unknown(GolemErrorUnknown { details: error }),
            })),
            TemplateError::AlreadyExists(template_id) => {
                WorkerError::BadRequest(Json(ErrorsBody {
                    errors: vec![format!("Template already exists: {template_id}")],
                }))
            }
            TemplateError::UnknownTemplateId(template_id) => {
                WorkerError::NotFound(Json(ErrorBody {
                    error: format!("Template not found: {template_id}"),
                }))
            }
            TemplateError::UnknownVersionedTemplateId(template_id) => {
                WorkerError::NotFound(Json(ErrorBody {
                    error: format!("Template not found: {template_id}"),
                }))
            }
            TemplateError::IOError(error) => WorkerError::InternalError(Json(GolemErrorBody {
                golem_error: GolemError::Unknown(GolemErrorUnknown { details: error }),
            })),
            TemplateError::TemplateProcessingError(error) => {
                WorkerError::InternalError(Json(GolemErrorBody {
                    golem_error: GolemError::Unknown(GolemErrorUnknown { details: error }),
                }))
            }
        }
    }
}

pub struct WorkerApi {
    pub template_service: Arc<dyn TemplateService + Sync + Send>,
    pub worker_service: Arc<dyn WorkerService + Sync + Send>,
}

#[OpenApi(prefix_path = "/templates", tag = ApiTags::Worker)]
impl WorkerApi {
    #[oai(path = "/workers/:worker_id", method = "get")]
    async fn get_worker_by_id(&self, worker_id: Path<String>) -> Result<Json<VersionedWorkerId>> {
        let worker_id: WorkerId = golem_common::model::WorkerId::from_str(&worker_id.0)?.into();
        let worker = self.worker_service.get_by_id(&worker_id).await?;

        Ok(Json(worker))
    }

    #[oai(path = "/:template_id/workers", method = "post")]
    async fn launch_new_worker(
        &self,
        template_id: Path<TemplateId>,
        request: Json<WorkerCreationRequest>,
    ) -> Result<Json<VersionedWorkerId>> {
        let template_id = template_id.0;
        let latest_template = self
            .template_service
            .get_latest_version(&template_id)
            .await
            .tap_err(|error| tracing::error!("Error getting latest template version: {:?}", error))?
            .ok_or(WorkerError::NotFound(Json(ErrorBody {
                error: format!("Template not found: {}", &template_id),
            })))?;

        let WorkerCreationRequest { name, args, env } = request.0;

        let worker_id = make_worker_id(template_id, name)?;

        let worker = self
            .worker_service
            .create(
                &worker_id,
                latest_template.versioned_template_id.version,
                args,
                env,
            )
            .await?;

        Ok(Json(worker))
    }

    #[oai(path = "/:template_id/workers/:worker_name", method = "delete")]
    async fn delete_worker(
        &self,
        template_id: Path<TemplateId>,
        worker_name: Path<String>,
    ) -> Result<Json<DeleteWorkerResponse>> {
        let worker_id = make_worker_id(template_id.0, worker_name.0)?;

        self.worker_service.delete(&worker_id).await?;

        Ok(Json(DeleteWorkerResponse {}))
    }

    #[oai(path = "/:template_id/workers/:worker_name/key", method = "post")]
    async fn get_invocation_key(
        &self,
        template_id: Path<TemplateId>,
        worker_name: Path<String>,
    ) -> Result<Json<InvocationKey>> {
        let worker_id = make_worker_id(template_id.0, worker_name.0)?;

        let invocation_key = self.worker_service.get_invocation_key(&worker_id).await?;

        Ok(Json(invocation_key))
    }

    #[oai(
        path = "/:template_id/workers/:worker_name/invoke-and-await",
        method = "post"
    )]
    async fn invoke_and_await_function(
        &self,
        template_id: Path<TemplateId>,
        worker_name: Path<String>,
        #[oai(name = "invocation-key")] invocation_key: Query<String>,
        function: Query<String>,
        #[oai(name = "calling-convention")] calling_convention: Query<Option<CallingConvention>>,
        params: Json<InvokeParameters>,
    ) -> Result<Json<InvokeResult>> {
        let worker_id = make_worker_id(template_id.0, worker_name.0)?;

        let calling_convention = calling_convention.0.unwrap_or(CallingConvention::Component);

        let result = self
            .worker_service
            .invoke_and_await_function(
                &worker_id,
                function.0,
                &InvocationKey {
                    value: invocation_key.0,
                },
                params.0.params,
                &calling_convention,
            )
            .await?;

        Ok(Json(InvokeResult { result }))
    }

    #[oai(path = "/:template_id/workers/:worker_name/invoke", method = "post")]
    async fn invoke_function(
        &self,
        template_id: Path<TemplateId>,
        worker_name: Path<String>,
        function: Query<String>,
        params: Json<InvokeParameters>,
    ) -> Result<Json<InvokeResponse>> {
        let worker_id = make_worker_id(template_id.0, worker_name.0)?;

        self.worker_service
            .invoke_function(&worker_id, function.0, params.0.params)
            .await?;

        Ok(Json(InvokeResponse {}))
    }

    #[oai(path = "/:template_id/workers/:worker_name/complete", method = "post")]
    async fn complete_promise(
        &self,
        template_id: Path<TemplateId>,
        worker_name: Path<String>,
        params: Json<CompleteParameters>,
    ) -> Result<Json<bool>> {
        let worker_id = make_worker_id(template_id.0, worker_name.0)?;
        let CompleteParameters { oplog_idx, data } = params.0;

        let result = self
            .worker_service
            .complete_promise(&worker_id, oplog_idx, data)
            .await?;

        Ok(Json(result))
    }

    #[oai(path = "/:template_id/workers/:worker_name/interrupt", method = "post")]
    async fn interrupt_worker(
        &self,
        template_id: Path<TemplateId>,
        worker_name: Path<String>,
        #[oai(name = "recovery-immediately")] recover_immediately: Query<Option<bool>>,
    ) -> Result<Json<InterruptResponse>> {
        let worker_id = make_worker_id(template_id.0, worker_name.0)?;

        self.worker_service
            .interrupt(&worker_id, recover_immediately.0.unwrap_or(false))
            .await?;

        Ok(Json(InterruptResponse {}))
    }

    #[oai(path = "/:template_id/workers/:worker_name", method = "get")]
    async fn get_worker_metadata(
        &self,
        template_id: Path<TemplateId>,
        worker_name: Path<String>,
    ) -> Result<Json<WorkerMetadata>> {
        let worker_id = make_worker_id(template_id.0, worker_name.0)?;
        let result = self.worker_service.get_metadata(&worker_id).await?;

        Ok(Json(result))
    }

    #[oai(path = "/:template_id/workers/:worker_name/resume", method = "post")]
    async fn resume_worker(
        &self,
        template_id: Path<TemplateId>,
        worker_name: Path<String>,
    ) -> Result<Json<ResumeResponse>> {
        let worker_id = make_worker_id(template_id.0, worker_name.0)?;

        self.worker_service.resume(&worker_id).await?;

        Ok(Json(ResumeResponse {}))
    }
}

fn make_worker_id(
    template_id: TemplateId,
    worker_name: String,
) -> std::result::Result<WorkerId, WorkerError> {
    WorkerId::new(template_id, worker_name).map_err(|error| {
        WorkerError::BadRequest(Json(ErrorsBody {
            errors: vec![format!("Invalid worker name: {error}")],
        }))
    })
}
