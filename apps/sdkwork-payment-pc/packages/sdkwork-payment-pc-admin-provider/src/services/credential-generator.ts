/**
 * Credential generator for quick debug-account setup.
 *
 * Two tiers:
 *   - `generateCredentials` (async, preferred): generates a REAL RSA-2048
 *     key pair through Web Crypto and exports it as PKCS8 private-key / SPKI
 *     public-key PEM blocks, plus provider-shaped secret values. Because the
 *     backend provider adapters parse these PEMs during initialization, the
 *     dry-run account test resolves the credentials and reports the adapter as
 *     initialized — the request chain reaches the provider adapter.
 *   - `generateFallbackCredentials` (sync fallback): structurally realistic
 *     values (provider-style key prefixes, well-formed PEM fences) for
 *     non-secure contexts and test environments where `crypto.subtle` is
 *     unavailable.
 *
 * Real providers require credentials issued by the PSP itself; generated keys
 * are for sandbox/development debugging only. The backend activation guard
 * still requires a successful dry-run test, so generated values cannot end up
 * in live payment routing by accident.
 */

import type { PaymentProviderCode } from "../types/provider-admin-types";

export interface GeneratedCredentialValues {
  primarySecret: string;
  webhookSecret?: string;
  certificate?: string;
  /** WeChat Pay public key ID (`PUB_KEY_ID_` prefix) for the official
   *  recommended WeChat Pay public key verification mode. */
  wechatpayPublicKeyId?: string;
}

const ALPHANUMERIC = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
const BASE64_CHARS = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/** Cryptographically-random characters with a Math.random fallback so the
 *  generator also works in non-secure contexts and test environments. */
function randomChars(length: number, alphabet: string): string {
  const bytes = new Uint32Array(length);
  if (typeof crypto !== "undefined" && typeof crypto.getRandomValues === "function") {
    crypto.getRandomValues(bytes);
    return Array.from(bytes, (value) => alphabet[value % alphabet.length]).join("");
  }
  let result = "";
  for (let index = 0; index < length; index += 1) {
    result += alphabet[Math.floor(Math.random() * alphabet.length)];
  }
  return result;
}

/** DER (binary) → standard base64-wrapped PEM block. */
function derToPem(der: ArrayBuffer, label: string): string {
  const bytes = new Uint8Array(der);
  let binary = "";
  const chunk = 0x8000;
  for (let offset = 0; offset < bytes.length; offset += chunk) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + chunk));
  }
  const body = btoa(binary).match(/.{1,64}/g)?.join("\n") ?? "";
  return `-----BEGIN ${label}-----\n${body}\n-----END ${label}-----`;
}

/** Generates a real RSA-2048 key pair and exports PKCS8/SPKI PEM blocks. */
async function generateRealKeyPair(): Promise<{ privatePem: string; publicPem: string }> {
  const keyPair = await crypto.subtle.generateKey(
    {
      name: "RSASSA-PKCS1-v1_5",
      modulusLength: 2048,
      publicExponent: new Uint8Array([1, 0, 1]),
      hash: "SHA-256",
    },
    true,
    ["sign", "verify"],
  );
  const [privateDer, publicDer] = await Promise.all([
    crypto.subtle.exportKey("pkcs8", keyPair.privateKey),
    crypto.subtle.exportKey("spki", keyPair.publicKey),
  ]);
  return {
    privatePem: derToPem(privateDer, "PRIVATE KEY"),
    publicPem: derToPem(publicDer, "PUBLIC KEY"),
  };
}

/**
 * Preferred entry point: real cryptographic keys when Web Crypto is available,
 * falling back to structurally realistic values otherwise. Always fills every
 * credential field appropriate for the provider.
 */
export async function generateCredentials(
  providerCode: PaymentProviderCode,
): Promise<GeneratedCredentialValues> {
  if (typeof crypto !== "undefined" && crypto.subtle && typeof crypto.subtle.generateKey === "function") {
    try {
      const { privatePem, publicPem } = await generateRealKeyPair();
      switch (providerCode) {
        case "stripe":
          // Stripe test secret keys are `sk_test_` + 24 characters.
          return {
            primarySecret: `sk_test_${randomChars(24, ALPHANUMERIC)}`,
            webhookSecret: `whsec_${randomChars(32, ALPHANUMERIC)}`,
          };
        case "alipay":
          // Alipay signs with an RSA2 merchant private key; the certificate
          // field holds the matching public key.
          return {
            primarySecret: privatePem,
            certificate: publicPem,
          };
        case "wechat_pay":
          return {
            primarySecret: privatePem,
            // WeChat Pay API v3 key is exactly 32 alphanumeric characters.
            webhookSecret: randomChars(32, ALPHANUMERIC),
            certificate: publicPem,
            // WeChat Pay public key ID (PUB_KEY_ID_ prefix) matched against
            // the Wechatpay-Serial header in the official public key mode.
            wechatpayPublicKeyId: `PUB_KEY_ID_${randomChars(32, "0123456789abcdef")}`,
          };
        default:
          // Sandbox requires only the primary credential and accepts any value.
          return { primarySecret: `sk_sandbox_${randomChars(24, ALPHANUMERIC)}` };
      }
    } catch {
      // Fall through to the structural fallback path (non-secure context etc.).
    }
  }
  return generateFallbackCredentials(providerCode);
}

/** Sync fallback: structurally plausible values without Web Crypto. */
export function generateFallbackCredentials(
  providerCode: PaymentProviderCode,
): GeneratedCredentialValues {
  switch (providerCode) {
    case "stripe":
      return {
        // Stripe test secret keys are `sk_test_` + 24 characters.
        primarySecret: `sk_test_${randomChars(24, ALPHANUMERIC)}`,
        webhookSecret: `whsec_${randomChars(32, ALPHANUMERIC)}`,
      };
    case "alipay":
      return {
        // Alipay signs with an RSA2 merchant private key; the certificate
        // field holds the matching public key.
        primarySecret: syntheticPem("RSA PRIVATE KEY", 16),
        certificate: syntheticPem("PUBLIC KEY", 8),
      };
    case "wechat_pay":
      return {
        primarySecret: syntheticPem("PRIVATE KEY", 16),
        // WeChat Pay API v3 key is exactly 32 alphanumeric characters.
        webhookSecret: randomChars(32, ALPHANUMERIC),
        certificate: syntheticPem("PUBLIC KEY", 8),
        wechatpayPublicKeyId: `PUB_KEY_ID_${randomChars(32, "0123456789abcdef")}`,
      };
    default:
      // Sandbox requires only the primary credential and accepts any value.
      return { primarySecret: `sk_sandbox_${randomChars(24, ALPHANUMERIC)}` };
  }
}

/** A structurally plausible PEM block: correct BEGIN/END fences with a
 *  base64-looking body wrapped at 64 characters per line. */
function syntheticPem(fence: string, lines: number): string {
  const body = Array.from(
    { length: lines },
    () => randomChars(64, BASE64_CHARS),
  ).join("\n");
  return `-----BEGIN ${fence}-----\n${body}\n-----END ${fence}-----`;
}
