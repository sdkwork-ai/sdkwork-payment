/**
 * Route contribution for the payment notify-domain admin surface.
 *
 * Base path `/admin/payments/notify-domains` aligns with the admin module
 * registry in `@sdkwork/payment-pc-admin-core` and the OpenAPI contract under
 * `/backend/v3/api/payments/notify_domains`.
 */

export const PAYMENT_PC_ADMIN_NOTIFY_DOMAIN_ROUTES = {
  moduleId: "payment-notify-domain",
  basePath: "/admin/payments/notify-domains",
  defaultPath: "/admin/payments/notify-domains",
  permissionPrefix: "commerce.payments.notify_domains",
  sections: {
    domains: {
      path: "/admin/payments/notify-domains",
      requiredPermissions: [
        "commerce.payments.notify_domains.list",
        "commerce.payments.notify_domains.create",
        "commerce.payments.notify_domains.update",
        "commerce.payments.notify_domains.delete",
      ],
    },
  },
} as const;

export type PaymentPcAdminNotifyDomainRouteManifest = typeof PAYMENT_PC_ADMIN_NOTIFY_DOMAIN_ROUTES;
