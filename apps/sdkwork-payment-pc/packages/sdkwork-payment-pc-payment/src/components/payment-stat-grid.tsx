import type { SdkworkPaymentStatusDigest } from "../payment";
import type { SdkworkPaymentStatistics } from "../payment-service";
import {
  createSdkworkPaymentToneStyle,
  type SdkworkPaymentVisualTone,
} from "../payment-appearance";
import { useSdkworkPaymentIntl } from "../payment-intl";

export interface SdkworkPaymentStatGridProps {
  digest: SdkworkPaymentStatusDigest;
  statistics: SdkworkPaymentStatistics;
}

export function SdkworkPaymentStatGrid({
  digest,
  statistics,
}: SdkworkPaymentStatGridProps) {
  const { copy } = useSdkworkPaymentIntl();
  const values = {
    ...digest,
    ...statistics,
  } as Record<string, number>;
  const statCards: Array<{
    description: string;
    key: keyof SdkworkPaymentStatistics | keyof SdkworkPaymentStatusDigest;
    label: string;
    tone: SdkworkPaymentVisualTone;
  }> = [
    {
      description: copy.stats.actionablePaymentsDescription,
      key: "actionablePayments",
      label: copy.stats.actionablePayments,
      tone: "warning",
    },
    {
      description: copy.stats.succeededCountDescription,
      key: "succeededCount",
      label: copy.stats.succeededCount,
      tone: "success",
    },
    {
      description: copy.stats.failedCountDescription,
      key: "failedCount",
      label: copy.stats.failedCount,
      tone: "danger",
    },
    {
      description: copy.stats.closedCountDescription,
      key: "closedCount",
      label: copy.stats.closedCount,
      tone: "neutral",
    },
  ];

  return (
    <section className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
      {statCards.map((card) => (
        <article
          className="rounded-[1.6rem] border border-[var(--sdk-color-border-default)] bg-[var(--sdk-color-surface-panel)] px-5 py-5 shadow-[var(--sdk-shadow-soft)]"
          key={card.key}
        >
          <div className="flex items-start justify-between gap-4">
            <div>
              <div className="text-[0.7rem] font-semibold uppercase tracking-[0.18em] text-[var(--sdk-color-text-muted)]">
                {card.label}
              </div>
              <div className="mt-3 text-4xl font-semibold tracking-tight text-[var(--sdk-color-text-primary)]">
                {values[card.key] ?? 0}
              </div>
            </div>
            <div
              className="mt-1 flex h-9 w-9 items-center justify-center rounded-[1rem] border"
              style={createSdkworkPaymentToneStyle(card.tone, {
                backgroundWeight: 18,
                borderWeight: 34,
              })}
            >
              <div className="h-2.5 w-2.5 rounded-full bg-current" />
            </div>
          </div>
          <p className="mt-2 text-sm leading-6 text-[var(--sdk-color-text-secondary)]">
            {card.description}
          </p>
        </article>
      ))}
    </section>
  );
}
