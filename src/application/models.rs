use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Product {
    pub id: i32,
    pub sku: Option<String>,
    pub name: Option<String>,
    pub price: f64,
    pub stock: i32,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicUser {
    pub id: i32,
    pub username: Option<String>,
    pub email: Option<String>,
    pub name: Option<String>,
    pub role: Option<String>,
    pub wallet_balance: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderItem {
    pub product_id: i32,
    pub sku: Option<String>,
    pub name: Option<String>,
    pub quantity: i32,
    pub price: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: i32,
    pub order_no: i32,
    pub user_id: i32,
    pub status: String,
    pub total: f64,
    pub paid_amount: f64,
    pub refunded: f64,
    pub coupon: Option<String>,
    pub created_at: Option<String>,
    pub items: Vec<OrderItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Balance {
    pub balance: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayResult {
    pub order_id: i32,
    pub status: String,
    pub method: String,
    pub bank_ref: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_serializes_with_public_contract_fields() {
        let product = Product {
            id: 7,
            sku: Some("SKU-7".into()),
            name: Some("Widget".into()),
            price: 12.5,
            stock: 3,
            created_at: Some("2026-11-12 00:00:00".into()),
        };
        let value = serde_json::to_value(product).unwrap();
        assert_eq!(value["id"], 7);
        assert_eq!(value["price"], 12.5);
        assert_eq!(value["created_at"], "2026-11-12 00:00:00");
    }
}
