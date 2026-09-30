import { useEffect } from "react";
import {
  CreditCard,
  QrCode,
  ReceiptText,
  ScanLine,
} from "lucide-react";
import {
  Button,
  EmptyState,
  LoadingBlock,
  Pagination,
  PaginationContent,
  PaginationItem,
  PaginationLink,
  PaginationNext,
  PaginationPrevious,
  StatusBadge,
  StatusNotice,
} from "@sdkwork/ui-pc-react";
import type { SdkworkPaymentFilter } from "../payment";
import type { SdkworkPaymentMessagesOverrides } from "../payment-copy";
import type { SdkworkPaymentController } from "../payment-controller";
import {
  useSdkworkPaymentController,
  useSdkworkPaymentControllerState,
} from "../payment-controller";
import {
  createSdkworkPaymentBackdropStyle,
  createSdkworkPaymentGlassStyle,
  createSdkworkPaymentHeroStyle,
  createSdkworkPaymentHeroTextStyle,
  createSdkworkPaymentPanelStyle,
  createSdkworkPaymentToneStyle,
} from "../payment-appearance";
import {
  SdkworkPaymentIntlProvider,
  useSdkworkPaymentIntl,
} from "../payment-intl";
import { SdkworkPaymentCreateDialog } from "../components/payment-create-dialog";
import { SdkworkPaymentDetailDrawer } from "../components/payment-detail-drawer";
import { SdkworkPaymentStatGrid } from "../components/payment-stat-grid";

const PAYMENT_PAGE_SIZE_OPTIONS = [10, 20, 50, 100] as const;

const PAGINATION_ELLIPSIS = -1;
const PAGINATION_MAX_VISIBLE = 5;

function buildPageNumbers(currentPage: number, totalPages: number): number[] {
  if (totalPages <= 1) {
    return [1];
  }
  if (totalPages <= PAGINATION_MAX_VISIBLE) {
    return Array.from({ length: totalPages }, (_, index) => index + 1);
  }
  const pages: number[] = [1];
  const startPage = Math.max(2, currentPage - 1);
  const endPage = Math.min(totalPages - 1, currentPage + 1);
  if (startPage > 2) {
    pages.push(PAGINATION_ELLIPSIS);
  }
  for (let page = startPage; page <= endPage; page++) {
    pages.push(page);
  }
  if (endPage < totalPages - 1) {
    pages.push(PAGINATION_ELLIPSIS);
  }
  pages.push(totalPages);
  return pages;
}

export interface SdkworkPaymentPageProps {
  controller?: SdkworkPaymentController;
  locale?: string | null;
  messages?: SdkworkPaymentMessagesOverrides;
}

interface SdkworkPaymentPageContentProps {
  controller?: SdkworkPaymentController;
  locale?: string | null;
  messages?: SdkworkPaymentMessagesOverrides;
}

function SdkworkPaymentPageContent({
  controller: controllerProp,
  locale,
  messages,
}: SdkworkPaymentPageContentProps) {
  const controller = useSdkworkPaymentController(controllerProp, {
    locale,
    messages,
  });
  const state = useSdkworkPaymentControllerState(controller);
  const {
    copy,
    formatCurrencyCny,
    formatPaginationSummary,
    formatRecommendedProductType,
    formatStatus,
    formatTimestamp,
  } = useSdkworkPaymentIntl();

  const filterOptions: Array<{
    filter: SdkworkPaymentFilter;
    label: string;
  }> = [
    {
      filter: "all",
      label: copy.filters.all,
    },
    {
      filter: "actionable",
      label: copy.filters.actionable,
    },
    {
      filter: "pending",
      label: copy.filters.pending,
    },
    {
      filter: "success",
      label: copy.filters.success,
    },
    {
      filter: "failed",
      label: copy.filters.failed,
    },
  ];
  const heroHighlights = [
    {
      icon: ReceiptText,
      label: copy.stats.totalAttempts,
      tone: "brand" as const,
      value: state.dashboard.statistics.totalCount,
    },
    {
      icon: QrCode,
      label: copy.stats.succeededCount,
      tone: "success" as const,
      value: state.dashboard.statistics.succeededCount,
    },
    {
      icon: ScanLine,
      label: copy.stats.pendingCount,
      tone: "warning" as const,
      value: state.dashboard.statistics.pendingCount,
    },
  ];

  useEffect(() => {
    if (!state.isBootstrapped && !state.isLoading && !state.lastError) {
      void controller.bootstrap().catch(() => undefined);
    }
  }, [controller, state.isBootstrapped, state.isLoading, state.lastError]);

  return (
    <div className="relative h-full overflow-y-auto">
      <div
        className="pointer-events-none absolute inset-x-0 top-0 h-80"
        style={createSdkworkPaymentBackdropStyle()}
      />

      <div className="relative px-4 py-4 sm:px-5 sm:py-5">
        <div className="mx-auto max-w-[92rem] space-y-5">
          <section className="grid gap-5 xl:grid-cols-[minmax(0,1.55fr)_minmax(20rem,0.95fr)]">
            <div
              className="overflow-hidden rounded-[2rem] border border-[color-mix(in_srgb,var(--sdk-color-border-default)_72%,transparent)] px-6 py-7 shadow-[var(--sdk-shadow-lg)]"
              style={{
                ...createSdkworkPaymentHeroStyle(),
                ...createSdkworkPaymentHeroTextStyle(),
              }}
            >
              <div className="flex flex-col gap-6 lg:flex-row lg:items-end lg:justify-between">
                <div className="max-w-3xl">
                  <div
                    className="inline-flex items-center gap-2 rounded-full border px-3 py-1 text-[0.7rem] font-semibold uppercase tracking-[0.18em] shadow-[var(--sdk-shadow-soft)]"
                    style={createSdkworkPaymentToneStyle("accent", {
                      backgroundWeight: 16,
                      borderWeight: 26,
                    })}
                  >
                    <ScanLine className="h-3.5 w-3.5" />
                    {copy.page.eyebrow}
                  </div>
                  <h1 className="mt-4 text-4xl font-semibold tracking-tight">{copy.page.title}</h1>
                  <p className="mt-3 text-sm leading-7" style={createSdkworkPaymentHeroTextStyle("muted")}>
                    {copy.page.description}
                  </p>
                </div>

                <div className="flex flex-wrap gap-3">
                  <Button
                    className="rounded-2xl px-5 py-5 text-sm font-semibold"
                    onClick={() => controller.openCreateDialog()}
                    type="button"
                    variant="secondary"
                  >
                    <CreditCard className="mr-2 h-4 w-4" />
                    {copy.actions.newPayment}
                  </Button>
                </div>
              </div>

              <div className="mt-8 grid gap-4 lg:grid-cols-3">
                {heroHighlights.map((highlight) => {
                  const Icon = highlight.icon;

                  return (
                    <div
                      className="rounded-[1.5rem] border p-5 shadow-[var(--sdk-shadow-sm)] backdrop-blur-xl"
                      key={highlight.label}
                      style={createSdkworkPaymentGlassStyle(highlight.tone, {
                        backgroundWeight: 14,
                        borderWeight: 26,
                      })}
                    >
                      <div className="flex items-center justify-between gap-4">
                        <div>
                          <div className="text-sm" style={createSdkworkPaymentHeroTextStyle("subtle")}>{highlight.label}</div>
                          <div className="mt-3 text-4xl font-semibold tracking-tight">
                            {highlight.value}
                          </div>
                        </div>
                        <div
                          className="flex h-12 w-12 items-center justify-center rounded-[1rem] border"
                          style={createSdkworkPaymentToneStyle(highlight.tone, {
                            backgroundWeight: 20,
                            borderWeight: 34,
                          })}
                        >
                          <Icon className="h-5 w-5" />
                        </div>
                      </div>
                    </div>
                  );
                })}
              </div>
            </div>

            <div
              className="rounded-[1.5rem] border p-5 shadow-[var(--sdk-shadow-soft)]"
              style={createSdkworkPaymentPanelStyle("neutral")}
            >
              <div className="text-[0.7rem] font-semibold uppercase tracking-[0.18em] text-[var(--sdk-color-text-muted)]">
                {copy.page.methodsEyebrow}
              </div>
              <h2 className="mt-2 text-xl font-semibold text-[var(--sdk-color-text-primary)]">{copy.page.methodsTitle}</h2>

              <div className="mt-5 space-y-3">
                {state.dashboard.methods.length === 0 ? (
                  <div className="rounded-[1.25rem] border border-dashed border-[var(--sdk-color-border-default)] px-4 py-6 text-sm text-[var(--sdk-color-text-secondary)]">
                    {copy.page.methodsEmpty}
                  </div>
                ) : state.dashboard.methods.map((method) => (
                  <button
                    className="w-full rounded-[1.25rem] border px-4 py-4 text-left transition-colors"
                    key={method.code}
                    onClick={() => controller.selectMethod(method.code)}
                    style={state.selectedMethodCode === method.code
                      ? createSdkworkPaymentPanelStyle("brand", {
                        backgroundWeight: 12,
                        borderWeight: 36,
                        surfaceColor: "var(--sdk-color-surface-panel-muted)",
                      })
                      : createSdkworkPaymentPanelStyle("neutral", {
                        backgroundWeight: 8,
                        borderWeight: 24,
                        surfaceColor: "var(--sdk-color-surface-panel-muted)",
                      })}
                    type="button"
                  >
                    <div className="flex items-center justify-between gap-3">
                      <div>
                        <div className="text-base font-semibold text-[var(--sdk-color-text-primary)]">
                          {method.label}
                        </div>
                        <div className="mt-1 text-sm text-[var(--sdk-color-text-secondary)]">
                          {formatRecommendedProductType(method.recommendedProductType)}
                        </div>
                      </div>
                      <div className="text-xs uppercase tracking-[0.14em] text-[var(--sdk-color-text-muted)]">
                        {method.code}
                      </div>
                    </div>
                  </button>
                ))}
              </div>
            </div>
          </section>

          <SdkworkPaymentStatGrid digest={state.dashboard.digest} statistics={state.dashboard.statistics} />

          {state.isLoading && !state.isBootstrapped ? <LoadingBlock label={copy.page.loading} /> : null}

          {state.lastError && !state.isCreateOpen ? (
            <StatusNotice title={copy.page.errorTitle} tone="danger">
              {state.lastError}
            </StatusNotice>
          ) : null}

          <section
            className="rounded-[1.5rem] border shadow-[var(--sdk-shadow-md)]"
            style={createSdkworkPaymentPanelStyle("neutral")}
          >
            <div className="border-b border-[var(--sdk-color-border-subtle)] px-6 py-5">
              <div className="flex flex-col gap-4 lg:flex-row lg:items-end lg:justify-between">
                <div>
                  <div className="text-xs font-semibold uppercase tracking-[0.18em] text-[var(--sdk-color-text-muted)]">{copy.page.recordsEyebrow}</div>
                  <h2 className="mt-2 text-xl font-semibold text-[var(--sdk-color-text-primary)]">{copy.page.recordsTitle}</h2>
                  <p className="mt-2 text-sm text-[var(--sdk-color-text-secondary)]">{copy.page.recordsDescription}</p>
                </div>

                <div className="inline-flex flex-wrap gap-2">
                  {filterOptions.map((filterOption) => (
                    <Button
                      key={filterOption.filter}
                      onClick={() => controller.setFilter(filterOption.filter)}
                      size="sm"
                      type="button"
                      variant={state.activeFilter === filterOption.filter ? "secondary" : "ghost"}
                    >
                      {filterOption.label}
                    </Button>
                  ))}
                </div>
              </div>
            </div>

            <div className="divide-y divide-[var(--sdk-color-border-subtle)]">
              {state.isRecordsLoading && state.visibleRecords.length === 0 ? (
                <div className="px-6 py-10">
                  <LoadingBlock label={copy.page.loading} />
                </div>
              ) : state.visibleRecords.length === 0 ? (
                <div className="px-6 py-10">
                  <EmptyState
                    description={copy.empty.paymentDescription}
                    title={copy.empty.paymentTitle}
                  />
                </div>
              ) : state.visibleRecords.map((record) => (
                <article className="flex flex-col gap-4 px-6 py-5 lg:flex-row lg:items-center lg:justify-between" key={record.id}>
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-3">
                      <div className="text-base font-semibold text-[var(--sdk-color-text-primary)]">
                        {record.paymentMethod || record.paymentProvider || copy.common.payment}
                      </div>
                      <StatusBadge label={formatStatus(record.status)} status={record.status} />
                    </div>
                    <div className="mt-2 flex flex-wrap items-center gap-3 text-sm text-[var(--sdk-color-text-secondary)]">
                      <span>{record.orderId || copy.common.emptyValue}</span>
                      <span>{formatTimestamp(record.createdAt)}</span>
                      <span>{record.outTradeNo || record.paymentSn || copy.common.emptyValue}</span>
                    </div>
                  </div>

                  <div className="flex flex-wrap items-center gap-4">
                    <div className="text-right">
                      <div className="font-semibold text-[var(--sdk-color-text-primary)]">
                        {formatCurrencyCny(record.amountCny)}
                      </div>
                      <div className="mt-1 text-xs uppercase tracking-[0.16em] text-[var(--sdk-color-text-muted)]">
                        {record.paymentProvider || copy.common.emptyValue}
                      </div>
                    </div>

                    <div className="flex flex-wrap gap-2">
                      <Button onClick={() => void controller.openDetail(record.id)} type="button" variant="outline">
                        {copy.actions.viewDetails}
                      </Button>
                    </div>
                  </div>
                </article>
              ))}
            </div>

            {state.pageInfo && state.pageInfo.totalItems > 0 ? (
              (() => {
                const pageInfo = state.pageInfo;
                return (
              <div className="flex flex-col gap-4 border-t border-[var(--sdk-color-border-subtle)] px-6 py-5 lg:flex-row lg:items-center lg:justify-between">
                <div className="flex flex-wrap items-center gap-3 text-sm text-[var(--sdk-color-text-secondary)]">
                  <span>{formatPaginationSummary(pageInfo)}</span>
                  <span className="text-[var(--sdk-color-border-default)]">|</span>
                  <label className="inline-flex items-center gap-2">
                    <span className="text-xs uppercase tracking-[0.14em] text-[var(--sdk-color-text-muted)]">
                      {copy.page.paginationLabel}
                    </span>
                    <select
                      className="rounded-[var(--sdk-radius-field)] border border-[var(--sdk-color-border-default)] bg-[var(--sdk-color-surface-panel)] px-3 py-1.5 text-sm text-[var(--sdk-color-text-primary)] outline-none focus:border-[var(--sdk-color-brand-primary)]"
                      onChange={(event) => void controller.setPageSize(Number(event.target.value))}
                      value={state.pageSize}
                    >
                      {PAYMENT_PAGE_SIZE_OPTIONS.map((option) => (
                        <option key={option} value={option}>{option}</option>
                      ))}
                    </select>
                  </label>
                </div>

                <Pagination className="mx-0 w-auto">
                  <PaginationContent>
                    <PaginationItem>
                      <PaginationPrevious
                        aria-disabled={state.page <= 1 || state.isRecordsLoading}
                        className={state.page <= 1 || state.isRecordsLoading ? "pointer-events-none opacity-50" : "cursor-pointer"}
                        onClick={(event) => {
                          event.preventDefault();
                          if (state.page > 1 && !state.isRecordsLoading) {
                            void controller.loadPage(state.page - 1);
                          }
                        }}
                      >
                        {copy.page.paginationPrevious}
                      </PaginationPrevious>
                    </PaginationItem>
                    {buildPageNumbers(state.page, pageInfo.totalPages).map((pageNumber, index) => (
                      <PaginationItem key={pageNumber === -1 ? `ellipsis-${index}` : pageNumber}>
                        {pageNumber === -1 ? (
                          <span className="px-2 text-[var(--sdk-color-text-muted)]">…</span>
                        ) : (
                          <PaginationLink
                            isActive={pageNumber === state.page}
                            onClick={(event) => {
                              event.preventDefault();
                              if (pageNumber !== state.page && !state.isRecordsLoading) {
                                void controller.loadPage(pageNumber);
                              }
                            }}
                          >
                            {pageNumber}
                          </PaginationLink>
                        )}
                      </PaginationItem>
                    ))}
                    <PaginationItem>
                      <PaginationNext
                        aria-disabled={!pageInfo.hasNextPage || state.isRecordsLoading}
                        className={!pageInfo.hasNextPage || state.isRecordsLoading ? "pointer-events-none opacity-50" : "cursor-pointer"}
                        onClick={(event) => {
                          event.preventDefault();
                          if (pageInfo.hasNextPage && !state.isRecordsLoading) {
                            void controller.loadPage(state.page + 1);
                          }
                        }}
                      >
                        {copy.page.paginationNext}
                      </PaginationNext>
                    </PaginationItem>
                  </PaginationContent>
                </Pagination>
              </div>
                );
              })()
            ) : null}
          </section>
        </div>
      </div>

      <SdkworkPaymentCreateDialog controller={controller} />
      <SdkworkPaymentDetailDrawer controller={controller} />
    </div>
  );
}

export function SdkworkPaymentPage({
  locale,
  messages,
  ...props
}: SdkworkPaymentPageProps) {
  const content = (
    <SdkworkPaymentPageContent
      {...props}
      locale={locale}
      messages={messages}
    />
  );

  if (locale || messages) {
    return (
      <SdkworkPaymentIntlProvider locale={locale} messages={messages}>
        {content}
      </SdkworkPaymentIntlProvider>
    );
  }

  return content;
}
