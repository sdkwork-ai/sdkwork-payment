/**
 * Route contribution for the payment dev-config admin surface.
 *
 * Base path `/admin/payments/devconfig` aligns with the admin module registry in
 * `@sdkwork/payment-pc-admin-core` (createSdkworkPaymentPcAdminModuleRegistry) and
 * the OpenAPI contracts under `/backend/v3/api/payments/certificates`,
 * `/backend/v3/api/payments/webhook_events`, and `/backend/v3/api/payments/dev/*`.
 */

export const PAYMENT_PC_ADMIN_DEVCONFIG_ROUTES = {
  moduleId: "payment-devconfig",
  basePath: "/admin/payments/devconfig",
  defaultPath: "/admin/payments/devconfig",
  permissionPrefix: "commerce.payments.devconfig",
  sections: {
    /**
     * One-cent test console. It creates a 0.01 payment, simulates settlement,
     * refunds through the production path and delivers a self-signed
     * notification, so it needs the whole dev-test permission family plus read
     * access to the accounts/channels it reports on.
     */
    testPayments: {
      path: "/admin/payments/devconfig/one-cent-test",
      requiredPermissions: [
        "commerce.payments.dev.test_payments",
        "commerce.payments.dev.sandbox_trigger",
        "commerce.payments.provider_accounts.read",
        "commerce.payments.methods.read",
        "commerce.payments.channels.read",
        "commerce.payments.refunds.read",
      ],
    },
    environment: {
      path: "/admin/payments/devconfig/environments",
      requiredPermissions: [
        "commerce.payments.provider_accounts.read",
        "commerce.payments.provider_accounts.update",
        "commerce.payments.provider_accounts.test",
      ],
    },
    webhook: {
      path: "/admin/payments/devconfig/webhook-debugger",
      requiredPermissions: [
        "commerce.payments.provider_accounts.read",
        "commerce.payments.webhook_events.read",
        "commerce.payments.dev.sandbox_trigger",
        "commerce.payments.dev.webhook_signature_test",
      ],
    },
    certificates: {
      path: "/admin/payments/devconfig/certificates",
      requiredPermissions: [
        "commerce.payments.certificates.read",
        "commerce.payments.certificates.create",
        "commerce.payments.certificates.delete",
      ],
    },
    logs: {
      path: "/admin/payments/devconfig/logs",
      requiredPermissions: [
        "commerce.payments.webhook_events.read",
        "commerce.payments.webhook_events.replay",
      ],
    },
  },
} as const;

export type PaymentPcAdminDevConfigRouteManifest = typeof PAYMENT_PC_ADMIN_DEVCONFIG_ROUTES;
