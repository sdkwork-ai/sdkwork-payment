import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SdkworkI18nProvider } from "@sdkwork/i18n-pc-react";

import {
  ProviderAccountFillInGuide,
  ProviderAccountFillInGuideLink,
} from "../src/components/ProviderAccountFillInGuide";

afterEach(cleanup);

function renderGuide(locale?: "en-US" | "zh-CN") {
  const guide = <ProviderAccountFillInGuide open onOpenChange={vi.fn()} />;
  return render(
    locale ? (
      <SdkworkI18nProvider locale={locale}>{guide}</SdkworkI18nProvider>
    ) : (
      guide
    ),
  );
}

describe("ProviderAccountFillInGuide localization", () => {
  it("renders English copy by default (en-US catalog)", () => {
    renderGuide();
    expect(screen.getByText("Fill-in guide")).toBeInTheDocument();
    expect(screen.getByText("Credentials — WeChat Pay")).toBeInTheDocument();
    expect(screen.getByText(/unique identifier for this account/u)).toBeInTheDocument();
    expect(screen.getByText(/WeChat Pay Public Key \(recommended\)/u)).toBeInTheDocument();
    expect(screen.getByText(/Wechatpay-Serial header carries this ID/u)).toBeInTheDocument();
  });

  it("renders Chinese copy under the zh-CN locale", () => {
    renderGuide("zh-CN");
    expect(screen.getByText("填写指南")).toBeInTheDocument();
    expect(screen.getByText("凭据 — 微信支付")).toBeInTheDocument();
    expect(screen.getByText(/账户唯一标识/u)).toBeInTheDocument();
    expect(screen.getByText(/微信支付公钥（推荐）/u)).toBeInTheDocument();
    expect(screen.getByText(/Wechatpay-Serial 响应头携带该公钥 ID/u)).toBeInTheDocument();
    expect(screen.getByText(/X.509 证书/u)).toBeInTheDocument();
    expect(screen.getByText(/在凭据字段填入证书 PEM/u)).toBeInTheDocument();
  });

  it("localizes the guide link label and title", () => {
    render(
      <SdkworkI18nProvider locale="zh-CN">
        <ProviderAccountFillInGuideLink onClick={vi.fn()} />
      </SdkworkI18nProvider>,
    );
    const link = screen.getByRole("button", { name: /填写指南/u });
    expect(link).toBeInTheDocument();
    expect(link.getAttribute("title")).toBe("打开填写指南");
  });
});
