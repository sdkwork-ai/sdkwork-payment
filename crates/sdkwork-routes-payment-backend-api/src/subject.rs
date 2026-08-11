use axum::Extension;
use sdkwork_iam_context_service::{AuthLevel, DeploymentMode, Environment, IamAppContext};

#[derive(Debug, Clone)]
pub(crate) struct AppRuntimeSubject {
    pub tenant_id: String,
    pub organization_id: Option<String>,
    pub user_id: String,
}

/// Backend handler entry enforcing IAM boundaries.
///
/// Both organization sessions and tenant (personal) sessions are accepted.
/// Tenant sessions — `organization_id` empty or `"0"`, i.e.
/// `LoginScope::Tenant` — are scoped to tenant-level payment rows
/// (`organization_id IS NULL` or `'0'`) by the query layer, matching how the
/// IAM and cloudrouter backend surfaces treat tenant logins. The backend
/// surface still requires an active member principal
/// (`IamAppContext::can_access_backend_api`).
pub(crate) fn backend_runtime_subject_from_extension(
    context: Option<Extension<IamAppContext>>,
) -> Result<AppRuntimeSubject, String> {
    let Some(Extension(context)) = context else {
        return Err("authenticated runtime context is required".to_owned());
    };

    if !context.can_access_backend_api() {
        return Err("principal is not permitted to access backend api surface".to_owned());
    }

    app_runtime_subject_from_iam(&context)
}

fn app_runtime_subject_from_iam(context: &IamAppContext) -> Result<AppRuntimeSubject, String> {
    let tenant_id = required_context_text(&context.tenant_id, "tenant_id")?;
    let organization_id = context
        .organization_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);

    Ok(AppRuntimeSubject {
        tenant_id,
        organization_id,
        user_id: required_context_text(&context.user_id, "user_id")?,
    })
}

fn required_context_text(value: &str, field_name: &'static str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(format!(
            "authenticated runtime context {field_name} is required"
        ));
    }
    Ok(value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdkwork_iam_context_service::{
        AuthLevel, DeploymentMode, Environment, IamAppContext, IamUserSurface, LoginScope,
    };

    fn member_context(organization_id: Option<&str>) -> IamAppContext {
        let mut context = IamAppContext::new(
            "100001",
            organization_id,
            "1",
            "session-1",
            "sdkwork-payment",
            Environment::Dev,
            DeploymentMode::Saas,
            AuthLevel::Password,
            vec!["tenant:100001".to_owned()],
            vec!["payment.intents.read".to_owned()],
        );
        // The user surface reflects an active organization membership (any
        // organization, including the platform root org `"0"`), which is what
        // grants backend API access for tenant sessions too.
        context.user_surface = IamUserSurface {
            app: true,
            organization_member: true,
        };
        context
    }

    #[test]
    fn tenant_session_is_accepted_with_tenant_level_scoping() {
        let context = member_context(None);
        assert_eq!(context.login_scope, LoginScope::Tenant);
        let subject = backend_runtime_subject_from_extension(Some(Extension(context)))
            .expect("tenant session must be accepted");
        assert_eq!(subject.tenant_id, "100001");
        assert_eq!(subject.organization_id, None);
        assert_eq!(subject.user_id, "1");
    }

    #[test]
    fn organization_session_is_accepted_with_organization_scoping() {
        let context = member_context(Some("100002"));
        assert_eq!(context.login_scope, LoginScope::Organization);
        let subject = backend_runtime_subject_from_extension(Some(Extension(context)))
            .expect("organization session must be accepted");
        assert_eq!(subject.organization_id.as_deref(), Some("100002"));
    }

    #[test]
    fn non_member_principal_is_rejected() {
        let mut context = member_context(None);
        context.user_surface = IamUserSurface {
            app: false,
            organization_member: false,
        };
        let error = backend_runtime_subject_from_extension(Some(Extension(context)))
            .expect_err("non-member principal must be rejected");
        assert!(error.contains("backend api surface"));
    }

    #[test]
    fn missing_runtime_context_is_rejected() {
        let error = backend_runtime_subject_from_extension(None)
            .expect_err("missing runtime context must be rejected");
        assert!(error.contains("runtime context"));
    }
}
