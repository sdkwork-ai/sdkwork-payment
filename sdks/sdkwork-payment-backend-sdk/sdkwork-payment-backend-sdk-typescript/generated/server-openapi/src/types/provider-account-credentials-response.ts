export interface ProviderAccountCredentialsResponse {
  code: 0;
  data: unknown & { item: { providerAccountId?: string; primarySecret?: string; webhookSecret?: string; certificate?: string; }; };
  /** Server-owned request correlation id. */
  traceId: string;
}
