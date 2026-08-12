import { appApiPath } from './paths';
import type { ApiRequestOptions, HttpClient } from '../http/client';

import type { CreatePaymentCommand, CreatePaymentIntentCommand, CreateRefundCommand, PageInfo, Payment, PaymentAttempt, PaymentIntent, PaymentMethod, PaymentRecord, PaymentStatistics, ReconcilePaymentCommand, Refund, SdkWorkCommandData } from '../types';


export interface CommerceRefundsListParams {
  page?: number;
  pageSize?: number;
  status?: string;
}

export class CommerceRefundsApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** List refunds. */
  async list(params?: CommerceRefundsListParams, requestOptions?: ApiRequestOptions): Promise<{ items: Refund[]; pageInfo: PageInfo; }> {
    const query = buildQueryString([
      { name: 'page', value: params?.page, style: 'form', explode: true, allowReserved: false },
      { name: 'page_size', value: params?.pageSize, style: 'form', explode: true, allowReserved: false },
      { name: 'status', value: params?.status, style: 'form', explode: true, allowReserved: false },
    ]);
    return this.client.request<{ items: Refund[]; pageInfo: PageInfo; }>(appendQueryString(appApiPath(`/refunds`), query), { signal: requestOptions?.signal, timeout: requestOptions?.timeout, method: 'GET' as any, sdkworkUnwrapKind: 'page' });
  }

/** Create a refund. */
  async create(body: CreateRefundCommand, requestOptions?: ApiRequestOptions): Promise<Refund> {
    return this.client.request<Refund>(appApiPath(`/refunds`), { signal: requestOptions?.signal, timeout: requestOptions?.timeout, method: 'POST' as any, body, contentType: 'application/json', sdkworkUnwrapKind: 'item' });
  }

/** Retrieve a refund. */
  async retrieve(refundId: string, requestOptions?: ApiRequestOptions): Promise<Refund> {
    return this.client.request<Refund>(appApiPath(`/refunds/${serializePathParameter(refundId, { name: 'refundId', style: 'simple', explode: false })}`), { signal: requestOptions?.signal, timeout: requestOptions?.timeout, method: 'GET' as any, sdkworkUnwrapKind: 'item' });
  }
}

export class CommercePaymentsStatusOutTradeNoApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** Retrieve payment status by provider trade number. */
  async retrieve(outTradeNo: string, requestOptions?: ApiRequestOptions): Promise<PaymentRecord> {
    return this.client.request<PaymentRecord>(appApiPath(`/payments/status/out_trade_no/${serializePathParameter(outTradeNo, { name: 'outTradeNo', style: 'simple', explode: false })}`), { signal: requestOptions?.signal, timeout: requestOptions?.timeout, method: 'GET' as any, sdkworkUnwrapKind: 'item' });
  }
}

export class CommercePaymentsStatusApi {
  private client: HttpClient;
  public readonly outTradeNo: CommercePaymentsStatusOutTradeNoApi;

  constructor(client: HttpClient) {
    this.client = client;
    this.outTradeNo = new CommercePaymentsStatusOutTradeNoApi(client);
  }


/** Retrieve payment status. */
  async retrieve(paymentId: string, requestOptions?: ApiRequestOptions): Promise<PaymentRecord> {
    return this.client.request<PaymentRecord>(appApiPath(`/payments/status/${serializePathParameter(paymentId, { name: 'paymentId', style: 'simple', explode: false })}`), { signal: requestOptions?.signal, timeout: requestOptions?.timeout, method: 'GET' as any, sdkworkUnwrapKind: 'item' });
  }
}

export class CommercePaymentsCheckoutApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** Retrieve payment checkout data. */
  async retrieve(paymentId: string, requestOptions?: ApiRequestOptions): Promise<Payment> {
    return this.client.request<Payment>(appApiPath(`/payments/checkout/${serializePathParameter(paymentId, { name: 'paymentId', style: 'simple', explode: false })}`), { signal: requestOptions?.signal, timeout: requestOptions?.timeout, method: 'GET' as any, sdkworkUnwrapKind: 'item' });
  }
}

export class CommercePaymentsStatisticsSummaryApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** Retrieve the payment statistics summary. */
  async retrieve(requestOptions?: ApiRequestOptions): Promise<PaymentStatistics> {
    return this.client.request<PaymentStatistics>(appApiPath(`/payments/statistics/summary`), { signal: requestOptions?.signal, timeout: requestOptions?.timeout, method: 'GET' as any, sdkworkUnwrapKind: 'item' });
  }
}

export class CommercePaymentsStatisticsApi {
  private client: HttpClient;
  public readonly summary: CommercePaymentsStatisticsSummaryApi;

  constructor(client: HttpClient) {
    this.client = client;
    this.summary = new CommercePaymentsStatisticsSummaryApi(client);
  }

}

export class CommercePaymentsAttemptsApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** Retrieve a payment attempt. */
  async retrieve(paymentAttemptId: string, requestOptions?: ApiRequestOptions): Promise<PaymentAttempt> {
    return this.client.request<PaymentAttempt>(appApiPath(`/payments/attempts/${serializePathParameter(paymentAttemptId, { name: 'paymentAttemptId', style: 'simple', explode: false })}`), { signal: requestOptions?.signal, timeout: requestOptions?.timeout, method: 'GET' as any, sdkworkUnwrapKind: 'item' });
  }
}

export interface CommercePaymentsRecordsListParams {
  page?: number;
  pageSize?: number;
  orderId?: string;
}

export class CommercePaymentsRecordsApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** List payment records. */
  async list(params?: CommercePaymentsRecordsListParams, requestOptions?: ApiRequestOptions): Promise<{ items: PaymentRecord[]; pageInfo: PageInfo; }> {
    const query = buildQueryString([
      { name: 'page', value: params?.page, style: 'form', explode: true, allowReserved: false },
      { name: 'page_size', value: params?.pageSize, style: 'form', explode: true, allowReserved: false },
      { name: 'order_id', value: params?.orderId, style: 'form', explode: true, allowReserved: false },
    ]);
    return this.client.request<{ items: PaymentRecord[]; pageInfo: PageInfo; }>(appendQueryString(appApiPath(`/payments/records`), query), { signal: requestOptions?.signal, timeout: requestOptions?.timeout, method: 'GET' as any, sdkworkUnwrapKind: 'page' });
  }

/** Retrieve a payment record. */
  async retrieve(paymentId: string, requestOptions?: ApiRequestOptions): Promise<PaymentRecord> {
    return this.client.request<PaymentRecord>(appApiPath(`/payments/records/${serializePathParameter(paymentId, { name: 'paymentId', style: 'simple', explode: false })}`), { signal: requestOptions?.signal, timeout: requestOptions?.timeout, method: 'GET' as any, sdkworkUnwrapKind: 'item' });
  }
}

export interface CommercePaymentsMethodsListParams {
  page?: number;
  pageSize?: number;
  clientType?: string;
}

export class CommercePaymentsMethodsApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** List available payment methods. */
  async list(params?: CommercePaymentsMethodsListParams, requestOptions?: ApiRequestOptions): Promise<{ items: PaymentMethod[]; pageInfo: PageInfo; }> {
    const query = buildQueryString([
      { name: 'page', value: params?.page, style: 'form', explode: true, allowReserved: false },
      { name: 'page_size', value: params?.pageSize, style: 'form', explode: true, allowReserved: false },
      { name: 'client_type', value: params?.clientType, style: 'form', explode: true, allowReserved: false },
    ]);
    return this.client.request<{ items: PaymentMethod[]; pageInfo: PageInfo; }>(appendQueryString(appApiPath(`/payments/methods`), query), { signal: requestOptions?.signal, timeout: requestOptions?.timeout, method: 'GET' as any, sdkworkUnwrapKind: 'page' });
  }
}

export class CommercePaymentsIntentsAttemptsApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** Create a payment attempt. */
  async create(paymentIntentId: string, requestOptions?: ApiRequestOptions): Promise<PaymentAttempt> {
    return this.client.request<PaymentAttempt>(appApiPath(`/payments/intents/${serializePathParameter(paymentIntentId, { name: 'paymentIntentId', style: 'simple', explode: false })}/attempts`), { signal: requestOptions?.signal, timeout: requestOptions?.timeout, method: 'POST' as any, sdkworkUnwrapKind: 'item' });
  }
}

export class CommercePaymentsIntentsApi {
  private client: HttpClient;
  public readonly attempts: CommercePaymentsIntentsAttemptsApi;

  constructor(client: HttpClient) {
    this.client = client;
    this.attempts = new CommercePaymentsIntentsAttemptsApi(client);
  }


/** Create a payment intent. */
  async create(body: CreatePaymentIntentCommand, requestOptions?: ApiRequestOptions): Promise<PaymentIntent> {
    return this.client.request<PaymentIntent>(appApiPath(`/payments/intents`), { signal: requestOptions?.signal, timeout: requestOptions?.timeout, method: 'POST' as any, body, contentType: 'application/json', sdkworkUnwrapKind: 'item' });
  }

/** Retrieve a payment intent. */
  async retrieve(paymentIntentId: string, requestOptions?: ApiRequestOptions): Promise<PaymentIntent> {
    return this.client.request<PaymentIntent>(appApiPath(`/payments/intents/${serializePathParameter(paymentIntentId, { name: 'paymentIntentId', style: 'simple', explode: false })}`), { signal: requestOptions?.signal, timeout: requestOptions?.timeout, method: 'GET' as any, sdkworkUnwrapKind: 'item' });
  }

/** Cancel a payment intent. */
  async cancel(paymentIntentId: string, requestOptions?: ApiRequestOptions): Promise<SdkWorkCommandData> {
    return this.client.request<SdkWorkCommandData>(appApiPath(`/payments/intents/${serializePathParameter(paymentIntentId, { name: 'paymentIntentId', style: 'simple', explode: false })}/cancel`), { signal: requestOptions?.signal, timeout: requestOptions?.timeout, method: 'POST' as any, sdkworkUnwrapKind: 'command' });
  }
}

export class CommercePaymentsApi {
  private client: HttpClient;
  public readonly intents: CommercePaymentsIntentsApi;
  public readonly methods: CommercePaymentsMethodsApi;
  public readonly records: CommercePaymentsRecordsApi;
  public readonly attempts: CommercePaymentsAttemptsApi;
  public readonly statistics: CommercePaymentsStatisticsApi;
  public readonly checkout: CommercePaymentsCheckoutApi;
  public readonly status: CommercePaymentsStatusApi;

  constructor(client: HttpClient) {
    this.client = client;
    this.intents = new CommercePaymentsIntentsApi(client);
    this.methods = new CommercePaymentsMethodsApi(client);
    this.records = new CommercePaymentsRecordsApi(client);
    this.attempts = new CommercePaymentsAttemptsApi(client);
    this.statistics = new CommercePaymentsStatisticsApi(client);
    this.checkout = new CommercePaymentsCheckoutApi(client);
    this.status = new CommercePaymentsStatusApi(client);
  }


/** Create a payment. */
  async create(body: CreatePaymentCommand, requestOptions?: ApiRequestOptions): Promise<Payment> {
    return this.client.request<Payment>(appApiPath(`/payments`), { signal: requestOptions?.signal, timeout: requestOptions?.timeout, method: 'POST' as any, body, contentType: 'application/json', sdkworkUnwrapKind: 'item' });
  }

/** Resolve the latest local payment record. */
  async reconcile(body: ReconcilePaymentCommand, requestOptions?: ApiRequestOptions): Promise<PaymentRecord> {
    return this.client.request<PaymentRecord>(appApiPath(`/payments/reconcile`), { signal: requestOptions?.signal, timeout: requestOptions?.timeout, method: 'POST' as any, body, contentType: 'application/json', sdkworkUnwrapKind: 'item' });
  }

/** Close a payment. */
  async close(paymentId: string, requestOptions?: ApiRequestOptions): Promise<SdkWorkCommandData> {
    return this.client.request<SdkWorkCommandData>(appApiPath(`/payments/${serializePathParameter(paymentId, { name: 'paymentId', style: 'simple', explode: false })}/close`), { signal: requestOptions?.signal, timeout: requestOptions?.timeout, method: 'POST' as any, sdkworkUnwrapKind: 'command' });
  }
}

export class CommerceApi {
  private client: HttpClient;
  public readonly payments: CommercePaymentsApi;
  public readonly refunds: CommerceRefundsApi;

  constructor(client: HttpClient) {
    this.client = client;
    this.payments = new CommercePaymentsApi(client);
    this.refunds = new CommerceRefundsApi(client);
  }

}

export function createCommerceApi(client: HttpClient): CommerceApi {
  return new CommerceApi(client);
}

function appendQueryString(path: string, rawQueryString: string): string {
  const query = rawQueryString.replace(/^\?+/, '');
  if (!query) {
    return path;
  }
  return path.includes('?') ? `${path}&${query}` : `${path}?${query}`;
}

interface PathParameterSpec {
  name: string;
  style: string;
  explode: boolean;
}

function serializePathParameter(value: unknown, spec: PathParameterSpec): string {
  if (value === undefined || value === null) {
    return '';
  }

  const style = spec.style || 'simple';
  if (Array.isArray(value)) {
    return serializePathArray(spec.name, value, style, spec.explode);
  }
  if (typeof value === 'object') {
    return serializePathObject(spec.name, value as Record<string, unknown>, style, spec.explode);
  }
  return pathPrefix(spec.name, style, false) + encodePathValue(serializePathPrimitive(value));
}

function serializePathArray(name: string, values: unknown[], style: string, explode: boolean): string {
  const serialized = values
    .filter((item) => item !== undefined && item !== null)
    .map((item) => encodePathValue(serializePathPrimitive(item)));
  if (serialized.length === 0) {
    return pathPrefix(name, style, false);
  }
  if (style === 'matrix') {
    return explode
      ? serialized.map((item) => `;${name}=${item}`).join('')
      : `;${name}=${serialized.join(',')}`;
  }
  return pathPrefix(name, style, false) + serialized.join(explode ? '.' : ',');
}

function serializePathObject(name: string, value: Record<string, unknown>, style: string, explode: boolean): string {
  const entries = Object.entries(value).filter(([, entryValue]) => entryValue !== undefined && entryValue !== null);
  if (entries.length === 0) {
    return pathPrefix(name, style, true);
  }
  if (style === 'matrix') {
    return explode
      ? entries.map(([key, entryValue]) => `;${encodePathValue(key)}=${encodePathValue(serializePathPrimitive(entryValue))}`).join('')
      : `;${name}=${entries.flatMap(([key, entryValue]) => [encodePathValue(key), encodePathValue(serializePathPrimitive(entryValue))]).join(',')}`;
  }
  const serialized = explode
    ? entries.map(([key, entryValue]) => `${encodePathValue(key)}=${encodePathValue(serializePathPrimitive(entryValue))}`).join(style === 'label' ? '.' : ',')
    : entries.flatMap(([key, entryValue]) => [encodePathValue(key), encodePathValue(serializePathPrimitive(entryValue))]).join(',');
  return pathPrefix(name, style, true) + serialized;
}

function pathPrefix(name: string, style: string, _objectValue: boolean): string {
  if (style === 'label') return '.';
  if (style === 'matrix') return `;${name}`;
  return '';
}

function encodePathValue(value: string): string {
  return encodeURIComponent(value);
}

function serializePathPrimitive(value: unknown): string {
  if (value instanceof Date) {
    return value.toISOString();
  }
  if (typeof value === 'object') {
    return JSON.stringify(value);
  }
  return String(value);
}
interface QueryParameterSpec {
  name: string;
  value: unknown;
  style: string;
  explode: boolean;
  allowReserved: boolean;
  contentType?: string;
}

function buildQueryString(parameters: QueryParameterSpec[]): string {
  const pairs: string[] = [];
  for (const parameter of parameters) {
    appendSerializedParameter(pairs, parameter);
  }
  return pairs.join('&');
}

function appendSerializedParameter(pairs: string[], parameter: QueryParameterSpec): void {
  if (parameter.value === undefined || parameter.value === null) {
    return;
  }

  if (parameter.contentType) {
    pairs.push(`${encodeQueryComponent(parameter.name)}=${encodeQueryValue(JSON.stringify(parameter.value), parameter.allowReserved)}`);
    return;
  }

  const style = parameter.style || 'form';
  if (style === 'deepObject') {
    appendDeepObjectParameter(pairs, parameter.name, parameter.value, parameter.allowReserved);
    return;
  }

  if (Array.isArray(parameter.value)) {
    appendArrayParameter(pairs, parameter.name, parameter.value, style, parameter.explode, parameter.allowReserved);
    return;
  }

  if (typeof parameter.value === 'object') {
    appendObjectParameter(pairs, parameter.name, parameter.value as Record<string, unknown>, style, parameter.explode, parameter.allowReserved);
    return;
  }

  pairs.push(`${encodeQueryComponent(parameter.name)}=${encodeQueryValue(serializePrimitive(parameter.value), parameter.allowReserved)}`);
}

function appendArrayParameter(
  pairs: string[],
  name: string,
  value: unknown[],
  style: string,
  explode: boolean,
  allowReserved: boolean,
): void {
  const values = value
    .filter((item) => item !== undefined && item !== null)
    .map((item) => serializePrimitive(item));
  if (values.length === 0) {
    return;
  }

  if (style === 'form' && explode) {
    for (const item of values) {
      pairs.push(`${encodeQueryComponent(name)}=${encodeQueryValue(item, allowReserved)}`);
    }
    return;
  }

  pairs.push(`${encodeQueryComponent(name)}=${encodeQueryValue(values.join(','), allowReserved)}`);
}

function appendObjectParameter(
  pairs: string[],
  name: string,
  value: Record<string, unknown>,
  style: string,
  explode: boolean,
  allowReserved: boolean,
): void {
  const entries = Object.entries(value).filter(([, entryValue]) => entryValue !== undefined && entryValue !== null);
  if (entries.length === 0) {
    return;
  }

  if (style === 'form' && explode) {
    for (const [key, entryValue] of entries) {
      pairs.push(`${encodeQueryComponent(key)}=${encodeQueryValue(serializePrimitive(entryValue), allowReserved)}`);
    }
    return;
  }

  const serialized = entries.flatMap(([key, entryValue]) => [key, serializePrimitive(entryValue)]).join(',');
  pairs.push(`${encodeQueryComponent(name)}=${encodeQueryValue(serialized, allowReserved)}`);
}

function appendDeepObjectParameter(
  pairs: string[],
  name: string,
  value: unknown,
  allowReserved: boolean,
): void {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    pairs.push(`${encodeQueryComponent(name)}=${encodeQueryValue(serializePrimitive(value), allowReserved)}`);
    return;
  }

  for (const [key, entryValue] of Object.entries(value as Record<string, unknown>)) {
    if (entryValue === undefined || entryValue === null) {
      continue;
    }
    pairs.push(`${encodeQueryComponent(`${name}[${key}]`)}=${encodeQueryValue(serializePrimitive(entryValue), allowReserved)}`);
  }
}

function serializePrimitive(value: unknown): string {
  if (value instanceof Date) {
    return value.toISOString();
  }
  if (typeof value === 'object') {
    return JSON.stringify(value);
  }
  return String(value);
}

function encodeQueryComponent(value: string): string {
  return encodeURIComponent(value);
}

function encodeQueryValue(value: string, allowReserved: boolean): string {
  const encoded = encodeURIComponent(value);
  if (!allowReserved) {
    return encoded;
  }
  return encoded.replace(/%3A/gi, ':')
    .replace(/%2F/gi, '/')
    .replace(/%3F/gi, '?')
    .replace(/%23/gi, '#')
    .replace(/%5B/gi, '[')
    .replace(/%5D/gi, ']')
    .replace(/%40/gi, '@')
    .replace(/%21/gi, '!')
    .replace(/%24/gi, '$')
    .replace(/%26/gi, '&')
    .replace(/%27/gi, "'")
    .replace(/%28/gi, '(')
    .replace(/%29/gi, ')')
    .replace(/%2A/gi, '*')
    .replace(/%2B/gi, '+')
    .replace(/%2C/gi, ',')
    .replace(/%3B/gi, ';')
    .replace(/%3D/gi, '=');
}
