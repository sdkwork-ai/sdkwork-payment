/**
 * Fill-in guide for the provider account create/edit dialog.
 *
 * A nested dialog reachable from a link button in the dialog header. Explains
 * each account field and how to obtain the credential material (PEM private
 * keys, public keys, certificates) for Alipay, WeChat Pay, and Stripe — the
 * same guidance a PSP console would surface next to its connection form.
 *
 * All copy is localized through the payment admin message catalog
 * (`usePaymentAdminMessages`), so the guide renders fully in the active
 * locale (zh-CN / en-US) instead of relying on DOM text replacement.
 */

import * as React from "react";
import { CircleHelp } from "lucide-react";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@sdkwork/ui-pc-react";
import { usePaymentAdminMessages } from "@sdkwork/payment-pc-admin-core";

export interface ProviderAccountFillInGuideProps {
  open: boolean;
  onOpenChange(open: boolean): void;
}

export function ProviderAccountFillInGuide(props: ProviderAccountFillInGuideProps) {
  const phrases = usePaymentAdminMessages().legacy.phrases;
  const t = (key: string) => phrases[key] ?? key;
  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("Fill-in guide")}</DialogTitle>
        </DialogHeader>
        <div className="max-h-[60dvh] space-y-4 overflow-y-auto pr-1 text-sm text-[var(--sdk-color-text-secondary)]">
          <GuideSection title={t("Account basics")}>
            <ul className="list-disc space-y-1 pl-4 text-xs leading-relaxed">
              <li>
                <strong className="font-medium text-[var(--sdk-color-text-primary)]">{t("Account No")}</strong>{" "}
                {t("— unique identifier for this account (e.g., stripe-live-primary). Cannot be changed after creation.")}
              </li>
              <li>
                <strong className="font-medium text-[var(--sdk-color-text-primary)]">{t("Merchant ID")}</strong>{" "}
                {t("— merchant/vendor id issued by the provider (Alipay PID, WeChat mch_id, Stripe acct_xxx).")}
              </li>
              <li>
                <strong className="font-medium text-[var(--sdk-color-text-primary)]">{t("Environment")}</strong>{" "}
                {t("— development, sandbox, or production.")}
              </li>
              <li>
                <strong className="font-medium text-[var(--sdk-color-text-primary)]">{t("Account Mode")}</strong>{" "}
                {t("— Direct (self-connection) or Partner / ISV (sub-merchants under a partner account).")}
              </li>
              <li>
                <strong className="font-medium text-[var(--sdk-color-text-primary)]">{t("Status")}</strong>{" "}
                {t("— create as Inactive, validate the credentials, then activate.")}
              </li>
            </ul>
          </GuideSection>

          <GuideSection title={t("Credentials — Alipay")}>
            <ul className="list-disc space-y-1 pl-4 text-xs leading-relaxed">
              <li>
                <strong className="font-medium text-[var(--sdk-color-text-primary)]">
                  {t("Merchant Private Key")}
                </strong>{" "}
                {t("— RSA2 application private key downloaded from Alipay Open Platform (key tool → generate key pair).")}
              </li>
              <li>
                <strong className="font-medium text-[var(--sdk-color-text-primary)]">{t("Alipay Public Key")}</strong>{" "}
                {t("— the platform's public key shown in the app console.")}
              </li>
              <li>
                <strong className="font-medium text-[var(--sdk-color-text-primary)]">{t("App ID")}</strong>{" "}
                {t("— from the application details on Alipay Open Platform (metadata section).")}
              </li>
            </ul>
          </GuideSection>

          <GuideSection title={t("Credentials — WeChat Pay")}>
            <ul className="list-disc space-y-1 pl-4 text-xs leading-relaxed">
              <li>
                <strong className="font-medium text-[var(--sdk-color-text-primary)]">
                  {t("WeChat Pay Merchant API Certificate")}
                </strong>{" "}
                {t("— apiclient_key.pem downloaded from the merchant platform (Account Center → API Security → Merchant API Certificate).")}
              </li>
              <li>
                <strong className="font-medium text-[var(--sdk-color-text-primary)]">{t("API v3 Key")}</strong>{" "}
                {t("— 32-character key configured in the merchant platform (API Security → APIv3 key setting); decrypts encrypted callbacks and platform certificates.")}
              </li>
              <li>
                <strong className="font-medium text-[var(--sdk-color-text-primary)]">
                  {t("Merchant Serial No")}
                </strong>{" "}
                {t("— certificate serial number shown next to the API certificate (metadata section); used in the request Authorization header serial_no field.")}
              </li>
              <li>
                <strong className="font-medium text-[var(--sdk-color-text-primary)]">
                  {t("Sign Verify Mode")}
                </strong>{" "}
                {t("— official API v3 two-option verification credential system:")}
                <ul className="mt-1 list-disc space-y-1 pl-4">
                  <li>
                    <strong className="font-medium text-[var(--sdk-color-text-primary)]">
                      {t("WeChat Pay Public Key (recommended)")}
                    </strong>{" "}
                    {t("— pub_key.pem from merchant platform (API Security → WeChat Pay Public Key → Apply). No expiry; new merchant numbers default to this mode. Fill the public key PEM in the credential field and its Public Key ID (PUB_KEY_ID_... prefix) in Provider Metadata. The Wechatpay-Serial header carries this ID.")}
                  </li>
                  <li>
                    <strong className="font-medium text-[var(--sdk-color-text-primary)]">
                      {t("Platform Certificate")}
                    </strong>{" "}
                    {t("— wechatpay_cert.pem (X.509, 5-year validity, rotate before expiry); obtainable via the platform certificate tool or GET /v3/certificates. Fill the certificate PEM in the credential field and its serial number in Provider Metadata. The Wechatpay-Serial header carries this serial.")}
                  </li>
                </ul>
              </li>
            </ul>
          </GuideSection>

          <GuideSection title={t("Credentials — Stripe")}>
            <ul className="list-disc space-y-1 pl-4 text-xs leading-relaxed">
              <li>
                <strong className="font-medium text-[var(--sdk-color-text-primary)]">{t("Secret Key")}</strong>{" "}
                {t("— sk_live_... / sk_test_... from Dashboard → Developers → API keys.")}
              </li>
              <li>
                <strong className="font-medium text-[var(--sdk-color-text-primary)]">
                  {t("Webhook Signing Secret")}
                </strong>{" "}
                {t("— whsec_... from the webhook endpoint details page.")}
              </li>
            </ul>
          </GuideSection>

          <GuideSection title={t("Notes")}>
            <ul className="list-disc space-y-1 pl-4 text-xs leading-relaxed">
              <li>
                {t("Credential values are write-only: after saving they are never shown again — the field displays Configured. Saving a replacement overwrites the stored value.")}
              </li>
              <li>
                {t("Each credential field accepts pasted PEM text or a local file via the Upload file link below the input. The file is read in the browser only and is sent to the server with the rest of the form.")}
              </li>
              <li>
                {t("Partner accounts manage sub-merchants (Alipay sub_appid / WeChat sub_mch_id / Stripe Connected Accounts) in the Sub-Merchants tab after creation.")}
              </li>
            </ul>
          </GuideSection>
        </div>
      </DialogContent>
    </Dialog>
  );
}

interface GuideSectionProps {
  title: string;
  children: React.ReactNode;
}

function GuideSection({ title, children }: GuideSectionProps) {
  return (
    <section>
      <h4 className="mb-1.5 text-xs font-semibold uppercase tracking-wider text-[var(--sdk-color-text-muted)]">
        {title}
      </h4>
      {children}
    </section>
  );
}

export interface ProviderAccountFillInGuideLinkProps {
  onClick(): void;
}

export function ProviderAccountFillInGuideLink(props: ProviderAccountFillInGuideLinkProps) {
  const phrases = usePaymentAdminMessages().legacy.phrases;
  const t = (key: string) => phrases[key] ?? key;
  return (
    <button
      type="button"
      onClick={props.onClick}
      title={t("Open the fill-in guide")}
      className="inline-flex items-center gap-1 text-xs font-medium text-[var(--sdk-color-brand-primary)] underline underline-offset-4 hover:text-[var(--sdk-color-brand-primary-hover)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--sdk-color-border-focus)] focus-visible:ring-offset-2 focus-visible:ring-offset-[var(--sdk-color-surface-canvas)]"
    >
      <CircleHelp className="h-3.5 w-3.5" aria-hidden="true" />
      {t("Fill-in guide")}
    </button>
  );
}
