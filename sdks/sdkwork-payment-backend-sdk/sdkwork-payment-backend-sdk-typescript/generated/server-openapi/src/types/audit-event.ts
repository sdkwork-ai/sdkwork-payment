export interface AuditEvent {
  id: string;
  tenantId: string;
  organizationId?: string;
  actorUserId: string;
  /** Stable verb, e.g. provider_account.create or payment_provider_credential.rotate. */
  action: string;
  resourceType: string;
  resourceId: string;
  /** Write-command request number that carried the change. */
  requestNo?: string;
  /** Non-secret context such as the changed credential kinds. */
  detail?: Record<string, unknown>;
  createdAt: string;
}
