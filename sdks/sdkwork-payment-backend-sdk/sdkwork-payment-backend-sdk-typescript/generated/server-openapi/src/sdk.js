import { createHttpClient } from './http/client';
import { createPaymentsApi } from './api/payments';
export class SdkworkBackendClient {
    httpClient;
    payments;
    constructor(config) {
        this.httpClient = createHttpClient(config);
        this.payments = createPaymentsApi(this.httpClient);
    }
    setAuthToken(token) {
        this.httpClient.setAuthToken(token);
        return this;
    }
    setAccessToken(token) {
        this.httpClient.setAccessToken(token);
        return this;
    }
    setTokenManager(manager) {
        this.httpClient.setTokenManager(manager);
        return this;
    }
    get http() {
        return this.httpClient;
    }
}
export function createClient(config) {
    return new SdkworkBackendClient(config);
}
export default SdkworkBackendClient;
//# sourceMappingURL=sdk.js.map