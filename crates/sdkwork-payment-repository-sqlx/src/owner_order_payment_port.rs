use crate::PostgresCommerceOwnerOrderPaymentStore;
use sdkwork_payment_service::{
    ConfirmOwnerOrderPaymentOutcome, OrderPaymentSettlementAttempt,
    OwnerOrderPaymentConfirmationFuture, OwnerOrderPaymentConfirmationPort,
};
impl OwnerOrderPaymentConfirmationPort for PostgresCommerceOwnerOrderPaymentStore {
    fn confirm_owner_order_payment<'a>(
        &'a self,
        attempt: &'a OrderPaymentSettlementAttempt,
    ) -> OwnerOrderPaymentConfirmationFuture<'a, ConfirmOwnerOrderPaymentOutcome> {
        Box::pin(async move {
            PostgresCommerceOwnerOrderPaymentStore::confirm_owner_order_payment(self, attempt).await
        })
    }
}
