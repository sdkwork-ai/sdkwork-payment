import { createPrivateKey, createPublicKey, webcrypto } from "node:crypto";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

import { generateCredentials, generateFallbackCredentials } from "../src/services/credential-generator";

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
    const values = await generateCredentials("alipay");
    expect(values.primarySecret).toMatch(/^-----BEGIN PRIVATE KEY-----\n[\s\S]+\n-----END PRIVATE KEY-----$/u);
    expect(values.certificate).toMatch(/^-----BEGIN PUBLIC KEY-----\n[\s\S]+\n-----END PUBLIC KEY-----$/u);
    // The private key must actually parse — this is what lets the backend
    // provider adapter initialize and the dry-run test reach the PSP.
    const privateKey = createPrivateKey(values.primarySecret);
    const publicKey = createPublicKey(values.certificate!);
    expect(privateKey.asymmetricKeyType).toBe("rsa");
    expect(publicKey.asymmetricKeyType).toBe("rsa");
  });

  it("generates a real RSA merchant private key for WeChat Pay", async () => {
    const values = await generateCredentials("wechat_pay");
    expect(createPrivateKey(values.primarySecret).asymmetricKeyType).toBe("rsa");
    expect(values.certificate).toMatch(/^-----BEGIN PUBLIC KEY-----/u);
    expect(values.webhookSecret).toMatch(ALPHANUMERIC);
    expect(values.webhookSecret).toHaveLength(32);
    // Official recommended verification mode: WeChat Pay public key ID with the
    // PUB_KEY_ID_ prefix matched against the Wechatpay-Serial header.
    expect(values.wechatpayPublicKeyId).toMatch(/^PUB_KEY_ID_[0-9a-f]{32}$/u);
  });

  it("produces distinct real keys on repeated calls", async () => {
    const first = await generateCredentials("alipay");
    const second = await generateCredentials("alipay");
    expect(first.primarySecret).not.toBe(second.primarySecret);
  });
});

describe("generateFallbackCredentials", () => {
  it("generates Stripe-shaped test secret and webhook signing secret", () => {
    const values = generateFallbackCredentials("stripe");
    expect(values.primarySecret).toMatch(/^sk_test_[A-Za-z0-9]{24}$/u);
    expect(values.webhookSecret).toMatch(/^whsec_[A-Za-z0-9]{32}$/u);
    expect(values.certificate).toBeUndefined();
  });

  it("generates well-formed PEM blocks for Alipay", () => {
    const values = generateFallbackCredentials("alipay");
    expect(values.primarySecret).toMatch(/^-----BEGIN RSA PRIVATE KEY-----\n[\s\S]+\n-----END RSA PRIVATE KEY-----$/u);
    expect(values.certificate).toMatch(/^-----BEGIN PUBLIC KEY-----\n[\s\S]+\n-----END PUBLIC KEY-----$/u);
    expect(values.webhookSecret).toBeUndefined();
  });

  it("generates WeChat-shaped API v3 key (exactly 32 chars) and PEM blocks", () => {
    const values = generateFallbackCredentials("wechat_pay");
    expect(values.primarySecret).toMatch(/^-----BEGIN PRIVATE KEY-----\n[\s\S]+\n-----END PRIVATE KEY-----$/u);
    // WeChat Pay public key mode: the verification key slot carries the SPKI
    // public key PEM (pub_key.pem shape) plus its PUB_KEY_ID_.
    expect(values.certificate).toMatch(/^-----BEGIN PUBLIC KEY-----\n[\s\S]+\n-----END PUBLIC KEY-----$/u);
    expect(values.wechatpayPublicKeyId).toMatch(/^PUB_KEY_ID_[0-9a-f]{32}$/u);
    expect(values.webhookSecret).toMatch(ALPHANUMERIC);
    expect(values.webhookSecret).toHaveLength(32);
  });

  it("generates a single primary credential for the sandbox provider", () => {
    const values = generateFallbackCredentials("sandbox");
    expect(values.primarySecret).toMatch(/^sk_sandbox_[A-Za-z0-9]{24}$/u);
    expect(values.webhookSecret).toBeUndefined();
    expect(values.certificate).toBeUndefined();
  });

  it("produces distinct values on repeated calls", () => {
    const first = generateFallbackCredentials("stripe");
    const second = generateFallbackCredentials("stripe");
    expect(first.primarySecret).not.toBe(second.primarySecret);
    expect(first.webhookSecret).not.toBe(second.webhookSecret);
  });
});
