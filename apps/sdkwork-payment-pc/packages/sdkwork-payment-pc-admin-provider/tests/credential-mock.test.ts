import { createPrivateKey, createPublicKey, webcrypto } from "node:crypto";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

import { generateCredentials, generateMockCredentials } from "../src/services/credential-mock";

const ALPHANUMERIC = /^[A-Za-z0-9]+$/u;

// jsdom does not implement crypto.subtle; inject Node's Web Crypto so the
// real-key path of generateCredentials is exercised.
const originalCrypto = globalThis.crypto;
beforeAll(() => {
  Object.defineProperty(globalThis, "crypto", { value: webcrypto, configurable: true });
});
afterAll(() => {
  Object.defineProperty(globalThis, "crypto", { value: originalCrypto, configurable: true });
});

describe("generateCredentials (real keys via Web Crypto)", () => {
  it("generates a real parseable RSA key pair for Alipay", async () => {
    const mock = await generateCredentials("alipay");
    expect(mock.primarySecret).toMatch(/^-----BEGIN PRIVATE KEY-----\n[\s\S]+\n-----END PRIVATE KEY-----$/u);
    expect(mock.certificate).toMatch(/^-----BEGIN PUBLIC KEY-----\n[\s\S]+\n-----END PUBLIC KEY-----$/u);
    // The private key must actually parse — this is what lets the backend
    // provider adapter initialize and the dry-run test reach the PSP.
    const privateKey = createPrivateKey(mock.primarySecret);
    const publicKey = createPublicKey(mock.certificate!);
    expect(privateKey.asymmetricKeyType).toBe("rsa");
    expect(publicKey.asymmetricKeyType).toBe("rsa");
  });

  it("generates a real RSA merchant private key for WeChat Pay", async () => {
    const mock = await generateCredentials("wechat_pay");
    expect(createPrivateKey(mock.primarySecret).asymmetricKeyType).toBe("rsa");
    expect(mock.certificate).toMatch(/^-----BEGIN PUBLIC KEY-----/u);
    expect(mock.webhookSecret).toMatch(ALPHANUMERIC);
    expect(mock.webhookSecret).toHaveLength(32);
  });

  it("produces distinct real keys on repeated calls", async () => {
    const first = await generateCredentials("alipay");
    const second = await generateCredentials("alipay");
    expect(first.primarySecret).not.toBe(second.primarySecret);
  });
});

describe("generateMockCredentials", () => {
  it("generates Stripe-shaped test secret and webhook signing secret", () => {
    const mock = generateMockCredentials("stripe");
    expect(mock.primarySecret).toMatch(/^sk_test_[A-Za-z0-9]{24}$/u);
    expect(mock.webhookSecret).toMatch(/^whsec_[A-Za-z0-9]{32}$/u);
    expect(mock.certificate).toBeUndefined();
  });

  it("generates well-formed PEM blocks for Alipay", () => {
    const mock = generateMockCredentials("alipay");
    expect(mock.primarySecret).toMatch(/^-----BEGIN RSA PRIVATE KEY-----\n[\s\S]+\n-----END RSA PRIVATE KEY-----$/u);
    expect(mock.certificate).toMatch(/^-----BEGIN PUBLIC KEY-----\n[\s\S]+\n-----END PUBLIC KEY-----$/u);
    expect(mock.webhookSecret).toBeUndefined();
  });

  it("generates WeChat-shaped API v3 key (exactly 32 chars) and PEM blocks", () => {
    const mock = generateMockCredentials("wechat_pay");
    expect(mock.primarySecret).toMatch(/^-----BEGIN PRIVATE KEY-----\n[\s\S]+\n-----END PRIVATE KEY-----$/u);
    expect(mock.certificate).toMatch(/^-----BEGIN CERTIFICATE-----\n[\s\S]+\n-----END CERTIFICATE-----$/u);
    expect(mock.webhookSecret).toMatch(ALPHANUMERIC);
    expect(mock.webhookSecret).toHaveLength(32);
  });

  it("generates a single primary credential for the sandbox provider", () => {
    const mock = generateMockCredentials("sandbox");
    expect(mock.primarySecret).toMatch(/^mock_secret_[A-Za-z0-9]{24}$/u);
    expect(mock.webhookSecret).toBeUndefined();
    expect(mock.certificate).toBeUndefined();
  });

  it("produces distinct values on repeated calls", () => {
    const first = generateMockCredentials("stripe");
    const second = generateMockCredentials("stripe");
    expect(first.primarySecret).not.toBe(second.primarySecret);
    expect(first.webhookSecret).not.toBe(second.webhookSecret);
  });
});
