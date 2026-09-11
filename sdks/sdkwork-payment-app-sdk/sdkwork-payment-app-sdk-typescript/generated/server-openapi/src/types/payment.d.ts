export interface Payment {
    paymentId: string;
    orderId: string;
    outTradeNo: string;
    amount: string;
    paymentMethod: string;
    status: string;
    statusName: string;
    paymentParams?: Record<string, string>;
    paymentUrl?: string;
}
//# sourceMappingURL=payment.d.ts.map