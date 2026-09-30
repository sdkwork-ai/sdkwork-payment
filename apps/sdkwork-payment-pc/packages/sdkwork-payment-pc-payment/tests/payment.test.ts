import { describe, expect, it } from "vitest";
import {
  type CreatePaymentWorkspaceManifestOptions,
  createPaymentRouteIntent,
  createPaymentWorkspaceManifest,
  type SdkworkPaymentMessagesOverrides,
  summarizeSdkworkPayments,
} from "../src";

describe("sdkwork-payment-pc-payment headless helpers", () => {
  it("creates payment workspace manifests and route intents aligned to the shared commerce contract", () => {
    const manifest = createPaymentWorkspaceManifest();
    const routeIntent = createPaymentRouteIntent({
      filter: "pending",
      orderId: "ORDER-9",
      paymentId: "1001",
    });

    expect(manifest).toMatchObject({
      capability: "payment",
      packageNames: [
        "@sdkwork/payment-pc-payment",
      ],
      routePath: "/payments",
    });
    expect(routeIntent).toEqual({
      filter: "pending",
      focusWindow: true,
      orderId: "ORDER-9",
      paymentId: "1001",
      route: "/payments?filter=pending&paymentId=1001&orderId=ORDER-9",
      source: "payment-workspace",
      type: "payment-route-intent",
    });
  });

  it("localizes payment workspace manifest defaults through the copy seam", () => {
    const manifest = createPaymentWorkspaceManifest({
      locale: "en-US",
      messages: {
        manifest: {
          description: "Localized manifest description",
          title: "Localized payment title",
        },
      } satisfies SdkworkPaymentMessagesOverrides,
    } satisfies CreatePaymentWorkspaceManifestOptions);

    expect(manifest).toMatchObject({
      description: "Localized manifest description",
      title: "Localized payment title",
    });
  });

  it("summarizes payment collections into reusable operational digests", () => {
    const summary = summarizeSdkworkPayments([
      {
        amountCny: 199,
        id: "PAY-1",
        status: "default",
      },
      {
        amountCny: 299,
        id: "PAY-2",
        status: "pending",
      },
      {
        amountCny: 399,
        id: "PAY-3",
        status: "success",
      },
      {
        amountCny: 99,
        id: "PAY-4",
        status: "failed",
      },
      {
        amountCny: 49,
        id: "PAY-5",
        status: "timeout",
      },
      {
        amountCny: 29,
        id: "PAY-6",
        status: "closed",
      },
    ]);

    expect(summary).toEqual({
      actionablePayments: 2,
      closedCount: 1,
      failedCount: 1,
      successfulPayments: 1,
      timedOutPayments: 1,
      totalAmountCny: 1074,
      totalCount: 6,
    });
  });
});
