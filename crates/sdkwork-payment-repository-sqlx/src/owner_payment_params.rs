use std::collections::BTreeMap;

use sdkwork_utils_rust::{build_commerce_cashier_url, commerce_cashier_scene};

/// Builds the payment params shared by owner-order pay outcomes.
///
/// `order_id` is the commerce order primary key carried by the cashier URL
/// (the H5 cashier page looks orders up with `orders.retrieve(orderId)`),
/// while `order_sn` remains available as the `orderSn` business order number.
pub fn owner_order_payment_params(
    provider_code: &str,
    order_id: &str,
    order_sn: &str,
    order_subject: Option<&str>,
    out_trade_no: &str,
) -> BTreeMap<String, String> {
    let scene = commerce_cashier_scene(order_subject);
    let cashier_url = build_commerce_cashier_url(scene, order_id, out_trade_no);
    let mut payment_params = BTreeMap::new();
    payment_params.insert(
        "providerCode".to_owned(),
        provider_code.to_ascii_lowercase(),
    );
    payment_params.insert("cashierUrl".to_owned(), cashier_url.clone());
    payment_params.insert("nextAction".to_owned(), "cashier".to_owned());
    payment_params.insert("orderSn".to_owned(), order_sn.to_owned());
    payment_params.insert("cashierScene".to_owned(), scene.to_owned());
    payment_params.insert("qrCodePayload".to_owned(), cashier_url);
    payment_params
}
