//! A binary whose derived types are private and used, as most services are:
//! a handle, check or module the derives generate and the program never
//! touches must not be dead code.

use flusso_query::{FlussoFragment, FlussoMultiDocument, FlussoRoot, FlussoValue};
use serde::Deserialize;

#[derive(Debug, Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    email: String,
    tier: Tier,
    orders: Vec<Order>,
    #[serde(rename = "billingAddress")]
    billing_address: Address,
}

#[derive(Debug, serde::Serialize, Deserialize, FlussoValue)]
#[serde(rename_all = "lowercase")]
#[flusso(keyword)]
enum Tier {
    Free,
    Pro,
    Enterprise,
}

#[derive(Debug, Deserialize, FlussoFragment)]
struct Order {
    status: String,
}

#[derive(Debug, Deserialize, FlussoFragment)]
struct Address {
    #[serde(flatten)]
    city: City,
}

#[derive(Debug, Deserialize, FlussoFragment)]
struct City {
    city: String,
}

#[derive(Debug, Deserialize, FlussoRoot)]
#[flusso(index = "products")]
struct Product {
    sku: String,
}

#[derive(Debug, FlussoMultiDocument)]
enum Hit {
    User(User),
    Product(Product),
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let user: User = serde_json::from_str(
        r#"{"email":"a@b.c","tier":"pro","orders":[{"status":"paid"}],
            "billingAddress":{"city":"Turin"}}"#,
    )?;
    let tier = match user.tier {
        Tier::Free => "free",
        Tier::Pro => "pro",
        Tier::Enterprise => "enterprise",
    };
    let statuses: Vec<&str> = user
        .orders
        .iter()
        .map(|order| order.status.as_str())
        .collect();
    let hit = <Hit as FlussoMultiDocument>::decode(
        <Product as FlussoRoot>::physical_index().as_str(),
        serde_json::json!({ "sku": "p1" }),
    )?;
    let sku = match hit {
        Hit::User(user) => user.email,
        Hit::Product(product) => product.sku,
    };
    if [
        user.email.as_str(),
        sku.as_str(),
        tier,
        &user.billing_address.city.city,
    ]
    .iter()
    .chain(&statuses)
    .any(|text| text.is_empty())
    {
        std::process::exit(1);
    }
    Ok(())
}
