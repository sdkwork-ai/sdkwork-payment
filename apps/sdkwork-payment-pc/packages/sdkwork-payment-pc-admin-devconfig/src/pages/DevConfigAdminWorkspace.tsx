/**
 * Dev config admin workspace.
 *
 * Five-tab workspace aligned with industry PSP admin consoles (Stripe
 * Dashboard → Developers, Alipay open platform → Dev config, WeChat Pay
 * merchant platform → Dev center):
 *
 *   0. One-cent test — provider × product coverage, a 0.01 wallet test for any
 *      configured product, settlement / status / close, refund and a
 *      self-signed notification delivery through the real pipeline
 *   1. Environment Switcher — switch provider account environments
 *      (development/sandbox/production) + credential test
 *   2. Webhook Debugger — sandbox trigger + signature verification
 *   3. Certificate Manager — PEM certificate reference CRUD + expiry warnings
 *   4. Integration Logs — full webhook event timeline + replay
 *
 * Uses an external store subscription pattern (subscribe/getState) so the host
 * app can wire it into React's useSyncExternalStore if needed.
 */

import * as React from "react";
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@sdkwork/ui-pc-react";
import {
  PaymentAdminI18nBoundary,
  PaymentAdminTabsContent,
  PaymentAdminTabsList,
  PaymentAdminTabsTrigger,
  PaymentAdminWorkspace,
  type PaymentBaseDataOption,
} from "@sdkwork/payment-pc-admin-core";
import { useDevConfigMessages } from "../i18n";
import { CertificateManager } from "../components/CertificateManager";
import { EnvironmentSwitcher } from "../components/EnvironmentSwitcher";
import { IntegrationLogs } from "../components/IntegrationLogs";
import { TestPaymentConsole } from "../components/TestPaymentConsole";
import { WebhookDebugger } from "../components/WebhookDebugger";
import type {
  PaymentCertificateDraft,
  PaymentDevConfigAdminController,
  PaymentDevConfigAdminSection,
  PaymentDevConfigAdminState,
  PaymentDevSandboxTriggerDraft,
  PaymentDevWebhookSignatureTestDraft,
  PaymentProviderEnvironment,
  PaymentTestNotificationDraft,
  PaymentTestPaymentDraft,
  PaymentWebhookEventListFilter,
} from "../types/devconfig-admin-types";

export interface PaymentDevConfigAdminWorkspaceProps {
  controller: PaymentDevConfigAdminController;
  title?: string;
  description?: string;
  section?: PaymentDevConfigAdminSection;
  /** Base-data options resolved by the host app; free-text fallback when empty. */
  currencyOptions?: readonly PaymentBaseDataOption[];
}

export function PaymentDevConfigAdminWorkspace(
  props: PaymentDevConfigAdminWorkspaceProps,
) {
  const { controller } = props;
  const m = useDevConfigMessages();
  const [state, setState] = React.useState<PaymentDevConfigAdminState>(() =>
    controller.getState(),
  );
  const [tab, setTab] = React.useState<PaymentDevConfigAdminSection>("testpayments");

  React.useEffect(() => {
    return controller.subscribe(() => {
      setState(controller.getState());
    });
  }, [controller]);

  React.useEffect(() => {
    void controller.load(props.section).then(setState).catch(() => {
      // error already surfaced via controller state.lastError
    });
  }, [controller, props.section]);

  // The test console needs the live coverage matrix; it is loaded once per
  // mount and refreshed explicitly after a checkout so configuration changes
  // show up without leaving the tab.
  React.useEffect(() => {
    if (props.section !== undefined && props.section !== "testpayments") {
      return;
    }
    void controller.loadTestProducts().catch(() => {
      // error already surfaced via controller state.lastError
    });
  }, [controller, props.section]);

  const busy =
    state.status === "loading" ||
    state.status === "saving" ||
    state.status === "testing";

  async function handleSwitchEnvironment(
    id: string,
    environment: PaymentProviderEnvironment,
  ) {
    await controller.switchProviderAccountEnvironment(id, environment);
  }

  async function handleTestProviderAccount(id: string) {
    await controller.testProviderAccount(id, { dryRun: false });
  }

  async function handleSandboxTrigger(
    providerAccountId: string,
    eventType: string,
    overrides: { amount?: string; currencyCode?: string; outTradeNo?: string },
  ) {
    const draft: PaymentDevSandboxTriggerDraft = {
      providerAccountId,
      eventType,
      ...overrides,
    };
    await controller.triggerSandboxEvent(draft);
  }

  async function handleSignatureTest(
    providerAccountId: string,
    payload: string,
    signature: string,
    timestamp: string,
    signatureHeader: string,
  ) {
    const draft: PaymentDevWebhookSignatureTestDraft = {
      providerAccountId,
      payload,
      signature,
      ...(timestamp ? { timestamp } : {}),
      ...(signatureHeader ? { signatureHeader } : {}),
    };
    await controller.testWebhookSignature(draft);
  }

  async function handleCreateCertificate(draft: PaymentCertificateDraft) {
    await controller.createCertificate(draft);
  }

  async function handleDeleteCertificate(id: string) {
    // Confirmation is handled inside CertificateManager via ConfirmDialog.
    await controller.deleteCertificate(id);
  }

  async function handleApplyWebhookFilter(filter: PaymentWebhookEventListFilter) {
    await controller.loadMoreWebhookEvents(filter);
  }

  async function handleReplayWebhook(eventId: string) {
    await controller.replayWebhookEvent(eventId);
  }

  async function handleReloadCoverage() {
    await controller.loadTestProducts();
  }

  async function handleCreateTestPayment(draft: PaymentTestPaymentDraft) {
    await controller.createTestPayment(draft);
  }

  async function handleCheckTestPayment(intentId: string) {
    await controller.checkTestPaymentStatus(intentId);
  }

  async function handleSettleTestPayment(intentId: string, outcome: "success" | "failed") {
    await controller.settleTestPayment(intentId, outcome);
  }

  async function handleCancelTestPayment(intentId: string) {
    await controller.cancelTestPayment(intentId);
  }

  async function handleRefundTestPayment(intentId: string) {
    await controller.refundTestPayment(intentId);
  }

  async function handleTestNotification(draft: PaymentTestNotificationDraft) {
    await controller.testNotification(draft);
  }

  const sections: Record<PaymentDevConfigAdminSection, React.ReactNode> = {
    testpayments: (
      <TestPaymentConsole
        products={state.testConsole.products}
        accounts={state.providerAccounts}
        results={state.testConsole}
        busy={busy}
        onReloadProducts={handleReloadCoverage}
        onCreateTestPayment={handleCreateTestPayment}
        onCheckStatus={handleCheckTestPayment}
        onSettle={handleSettleTestPayment}
        onCancel={handleCancelTestPayment}
        onRefund={handleRefundTestPayment}
        onTestNotification={handleTestNotification}
      />
    ),
    environment: (
      <EnvironmentSwitcher
        accounts={state.providerAccounts}
        pageInfo={state.listPageInfo?.providerAccounts}
        busy={busy}
        lastTestResult={state.lastTestResult}
        onSwitchEnvironment={handleSwitchEnvironment}
        onTest={handleTestProviderAccount}
        onLoadMore={() => void controller.loadMoreProviderAccounts()}
      />
    ),
    webhook: (
      <WebhookDebugger
        accounts={state.providerAccounts}
        recentEvents={state.webhookEvents}
        busy={busy}
        lastSandboxTriggerResult={state.lastSandboxTriggerResult}
        lastSignatureTestResult={state.lastSignatureTestResult}
        currencyOptions={props.currencyOptions}
        onSandboxTrigger={handleSandboxTrigger}
        onSignatureTest={handleSignatureTest}
      />
    ),
    certificates: (
      <CertificateManager
        certificates={state.certificates}
        pageInfo={state.listPageInfo?.certificates}
        busy={busy}
        onCreate={handleCreateCertificate}
        onDelete={handleDeleteCertificate}
        onLoadMore={() => void controller.loadMoreCertificates()}
      />
    ),
    logs: (
      <IntegrationLogs
        events={state.webhookEvents}
        pageInfo={state.listPageInfo?.webhookEvents}
        busy={busy}
        lastReplayResult={state.lastReplayResult}
        onApplyFilter={handleApplyWebhookFilter}
        onLoadMore={() => void controller.loadMoreWebhookEvents()}
        onReplay={handleReplayWebhook}
      />
    ),
  };

  return (
    <PaymentAdminI18nBoundary>
      <PaymentAdminWorkspace
        data-slot="payment-devconfig-admin-workspace"
        description={props.description}
        error={state.lastError}
        title={props.title ?? m.workspace.defaultTitle}
      >
        {props.section ? (
          sections[props.section]
        ) : (
          <Tabs
            value={tab}
            onValueChange={(value) =>
              setTab(value as PaymentDevConfigAdminSection)
            }
          >
            <PaymentAdminTabsList aria-label={m.workspace.sectionsAriaLabel}>
              <PaymentAdminTabsTrigger value="testpayments">
                {m.workspace.tabTestPayments}
              </PaymentAdminTabsTrigger>
              <PaymentAdminTabsTrigger value="environment">
                {m.workspace.tabEnvironment}
              </PaymentAdminTabsTrigger>
              <PaymentAdminTabsTrigger value="webhook">
                {m.workspace.tabWebhook}
              </PaymentAdminTabsTrigger>
              <PaymentAdminTabsTrigger value="certificates">
                {m.workspace.tabCertificates}
              </PaymentAdminTabsTrigger>
              <PaymentAdminTabsTrigger value="logs">
                {m.workspace.tabLogs}
              </PaymentAdminTabsTrigger>
            </PaymentAdminTabsList>

            <PaymentAdminTabsContent value="testpayments">
              {sections.testpayments}
            </PaymentAdminTabsContent>

            <PaymentAdminTabsContent value="environment">
              {sections.environment}
            </PaymentAdminTabsContent>

            <PaymentAdminTabsContent value="webhook">
              {sections.webhook}
            </PaymentAdminTabsContent>

            <PaymentAdminTabsContent value="certificates">
              {sections.certificates}
            </PaymentAdminTabsContent>

            <PaymentAdminTabsContent value="logs">
              {sections.logs}
            </PaymentAdminTabsContent>
          </Tabs>
        )}
      </PaymentAdminWorkspace>
    </PaymentAdminI18nBoundary>
  );
}

// Re-export commonly used Tabs sub-components for host apps that want to wrap them.
export { Tabs as PaymentDevConfigAdminTabs, TabsList, TabsTrigger, TabsContent };
