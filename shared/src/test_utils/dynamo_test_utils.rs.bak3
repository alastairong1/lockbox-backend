use aws_sdk_dynamodb::types::{
    AttributeDefinition, AttributeValue, GlobalSecondaryIndex, IndexStatus, KeySchemaElement,
    KeyType, Projection, ProjectionType, ProvisionedThroughput, ScalarAttributeType, TableStatus,
};
use aws_sdk_dynamodb::Client;
use std::error::Error;
// Use log macros, but ensure test_logging::init_test_logging() is called in test files
use log::{debug, error, info};
// Reference to our test logging initialization
// use super::test_logging;

/// # DynamoDB test utilities
///
/// These utilities help set up and manage DynamoDB tables for testing.
///
/// ## Logging
/// This module uses the standard `log` crate macros for logging, but requires
/// test_logging::init_test_logging() to be called in your test file for logs to appear.
///
/// ## Example
/// ```rust
/// use lockbox_shared::test_utils::test_logging::init_test_logging;
/// use lockbox_shared::test_utils::dynamo_test_utils;
///
/// #[tokio::test]
/// async fn my_dynamo_test() {
///     // Initialize test logging
///     init_test_logging();
///     
///     // Now the log messages from dynamo_test_utils will be displayed
///     // based on the LOG_LEVEL environment variable
///     let client = dynamo_test_utils::create_dynamo_client().await;
///     // ...
/// }
/// ```

// Constants for DynamoDB tests
pub const DYNAMO_LOCAL_URI: &str = "http://localhost:8000";

// Helper to check if DynamoDB integration tests should be used
pub fn use_dynamodb() -> bool {
    std::env::var("USE_DYNAMODB").unwrap_or_default() == "true"
}

// Helper to set up a DynamoDB client for local testing
pub async fn create_dynamo_client() -> Client {
    let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .endpoint_url(DYNAMO_LOCAL_URI)
        .load()
        .await;

    Client::new(&config)
}

// Helper to create a table with the given name and GSIs
// Note: Make sure to call test_utils::test_logging::init_test_logging() in your test file
// for these logs to be properly configured and displayed.
pub async fn create_dynamo_table(
    client: &Client,
    table_name: &str,
    gsi_configs: Vec<(&str, &str, KeyType)>,
) -> Result<(), Box<dyn Error>> {
    info!("Creating dynamo table '{}' with GSIs...", table_name);

    // Check if table already exists
    let tables = client.list_tables().send().await?;
    let table_names = tables.table_names();
    if table_names.contains(&table_name.to_string()) {
        info!(
            "Table '{}' already exists, deleting it first...",
            table_name
        );
        // Delete table if it exists
        client.delete_table().table_name(table_name).send().await?;
        // Wait for table deletion to complete
        loop {
            let tables = client.list_tables().send().await?;
            if !tables.table_names().contains(&table_name.to_string()) {
                info!("Table '{}' successfully deleted!", table_name);
                break;
            }
            debug!("Table '{}' still exists, waiting...", table_name);
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }

    // Define primary key (always using 'id' as the hash key)
    let id_key = KeySchemaElement::builder()
        .attribute_name("id")
        .key_type(KeyType::Hash)
        .build()?;

    // Define id attribute
    let id_attr = AttributeDefinition::builder()
        .attribute_name("id")
        .attribute_type(ScalarAttributeType::S)
        .build()?;

    // Create the table create request with primary key
    let mut create_table_req = client
        .create_table()
        .table_name(table_name)
        .key_schema(id_key)
        .attribute_definitions(id_attr);

    // Add all GSI attributes and indices
    let mut attribute_definitions = Vec::new();
    let mut global_secondary_indices = Vec::new();

    for (gsi_name, attr_name, key_type) in gsi_configs {
        // Add attribute definition
        let attr_def = AttributeDefinition::builder()
            .attribute_name(attr_name)
            .attribute_type(ScalarAttributeType::S)
            .build()?;
        attribute_definitions.push(attr_def);

        // Create GSI
        let key_schema = KeySchemaElement::builder()
            .attribute_name(attr_name)
            .key_type(key_type)
            .build()?;

        let gsi = GlobalSecondaryIndex::builder()
            .index_name(gsi_name)
            .key_schema(key_schema)
            .projection(
                Projection::builder()
                    .projection_type(ProjectionType::All)
                    .build(),
            )
            .provisioned_throughput(
                ProvisionedThroughput::builder()
                    .read_capacity_units(5)
                    .write_capacity_units(5)
                    .build()?,
            )
            .build()?;

        global_secondary_indices.push(gsi);
    }

    // Add all attribute definitions to the request
    for attr_def in attribute_definitions {
        create_table_req = create_table_req.attribute_definitions(attr_def);
    }

    // Add all GSIs to the request
    for gsi in global_secondary_indices {
        create_table_req = create_table_req.global_secondary_indexes(gsi);
    }

    // Add provisioned throughput
    create_table_req = create_table_req.provisioned_throughput(
        ProvisionedThroughput::builder()
            .read_capacity_units(5)
            .write_capacity_units(5)
            .build()?,
    );

    // Create the table
    info!("Creating table '{}'...", table_name);
    create_table_req.send().await?;

    // Wait for the table (and GSIs) to become ACTIVE before running tests
    info!("Waiting for table '{}' to become ACTIVE...", table_name);
    loop {
        let resp = client
            .describe_table()
            .table_name(table_name)
            .send()
            .await?;
        if let Some(table_desc) = resp.table() {
            if table_desc.table_status() == Some(&TableStatus::Active) {
                // ensure all global secondary indexes are active
                let gsi_descs = table_desc.global_secondary_indexes();
                if gsi_descs.is_empty()
                    || gsi_descs
                        .iter()
                        .all(|idx| idx.index_status() == Some(&IndexStatus::Active))
                {
                    info!("Table '{}' and all GSIs are now ACTIVE!", table_name);
                    break;
                } else {
                    debug!("Table '{}' is ACTIVE but waiting for GSIs...", table_name);
                }
            } else {
                debug!(
                    "Table '{}' status: {:?}",
                    table_name,
                    table_desc.table_status()
                );
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    info!("Table '{}' is ready for testing!", table_name);
    Ok(())
}

// Helper to clean the DynamoDB table between tests
pub async fn clear_dynamo_table(client: &Client, table_name: &str) -> Result<(), Box<dyn Error>> {
    let mut last_key = None;
    loop {
        let scan_resp_result = client
            .scan()
            .table_name(table_name)
            .set_exclusive_start_key(last_key.clone())
            .send()
            .await;

        let scan_resp = match scan_resp_result {
            Ok(output) => output,
            Err(e) => {
                error!("Failed to scan table '{}': {}", table_name, e);
                // Propagate the error, boxing it
                return Err(Box::new(e));
            }
        };

        if let Some(items) = scan_resp.items {
            for item in &items {
                if let Some(id) = item.get("id") {
                    if let Some(id_str) = id.as_s().ok() {
                        let _ = client
                            .delete_item()
                            .table_name(table_name)
                            .key("id", AttributeValue::S(id_str.to_string()))
                            .send()
                            .await
                            .map_err(|e| {
                                error!(
                                    "Failed to delete item '{}' from table '{}': {}",
                                    id_str, table_name, e
                                );
                                // Box the error before propagation via ?
                                Box::new(e) as Box<dyn Error>
                            })?;
                    }
                }
            }
        }

        last_key = scan_resp.last_evaluated_key;
        if last_key.is_none() {
            break; // No more items to scan
        }
    }
    Ok(())
}

// Helper to create the invitation table for testing
pub async fn create_invitation_table(
    client: &Client,
    table_name: &str,
) -> Result<(), Box<dyn Error>> {
    info!("Creating invitation table '{}'...", table_name);

    let gsi_configs = vec![
        ("box_id-index", "boxId", KeyType::Hash),
        ("invite_code-index", "inviteCode", KeyType::Hash),
        ("creatorId-index", "creatorId", KeyType::Hash),
    ];

    create_dynamo_table(client, table_name, gsi_configs).await
}

// Helper to create the box table for testing
pub async fn create_box_table(client: &Client, table_name: &str) -> Result<(), Box<dyn Error>> {
    info!("Creating box table '{}' for testing...", table_name);

    // Check if table already exists
    let tables = client.list_tables().send().await?;
    let table_names = tables.table_names();

    if table_names.contains(&table_name.to_string()) {
        info!(
            "Table '{}' already exists, deleting it first...",
            table_name
        );
        // Delete table if it exists
        match client.delete_table().table_name(table_name).send().await {
            Ok(_) => info!("Successfully deleted existing table '{}'", table_name),
            Err(e) => error!("Error deleting table '{}': {}", table_name, e),
        }

        // Wait for table deletion to complete
        info!("Waiting for table '{}' to be deleted...", table_name);
        loop {
            let tables = client.list_tables().send().await?;
            if !tables.table_names().contains(&table_name.to_string()) {
                info!("Table '{}' successfully deleted!", table_name);
                break;
            }
            debug!("Table '{}' still exists, waiting...", table_name);
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }

    info!("Creating new table '{}'...", table_name);

    // Define GSI configurations
    let gsi_configs = vec![("owner_id-index", "ownerId", KeyType::Hash)];

    // Define primary key (always using 'id' as the hash key)
    let id_key = KeySchemaElement::builder()
        .attribute_name("id")
        .key_type(KeyType::Hash)
        .build()?;

    // Define id attribute
    let id_attr = AttributeDefinition::builder()
        .attribute_name("id")
        .attribute_type(ScalarAttributeType::S)
        .build()?;

    // Create the table create request with primary key
    let mut create_table_req = client
        .create_table()
        .table_name(table_name)
        .key_schema(id_key)
        .attribute_definitions(id_attr);

    // Add all GSI attributes and indices
    let mut attribute_definitions = Vec::new();
    let mut global_secondary_indices = Vec::new();

    for (gsi_name, attr_name, key_type) in gsi_configs {
        // Add attribute definition
        let attr_def = AttributeDefinition::builder()
            .attribute_name(attr_name)
            .attribute_type(ScalarAttributeType::S)
            .build()?;
        attribute_definitions.push(attr_def);

        // Create GSI
        let key_schema = KeySchemaElement::builder()
            .attribute_name(attr_name)
            .key_type(key_type)
            .build()?;

        let gsi = GlobalSecondaryIndex::builder()
            .index_name(gsi_name)
            .key_schema(key_schema)
            .projection(
                Projection::builder()
                    .projection_type(ProjectionType::All)
                    .build(),
            )
            .provisioned_throughput(
                ProvisionedThroughput::builder()
                    .read_capacity_units(5)
                    .write_capacity_units(5)
                    .build()?,
            )
            .build()?;

        global_secondary_indices.push(gsi);
    }

    // Add all attribute definitions to the request
    for attr_def in attribute_definitions {
        create_table_req = create_table_req.attribute_definitions(attr_def);
    }

    // Add all GSIs to the request
    for gsi in global_secondary_indices {
        create_table_req = create_table_req.global_secondary_indexes(gsi);
    }

    // Add provisioned throughput
    create_table_req = create_table_req.provisioned_throughput(
        ProvisionedThroughput::builder()
            .read_capacity_units(5)
            .write_capacity_units(5)
            .build()?,
    );

    // Create the table
    info!("Sending create table request...");
    let create_result = create_table_req.send().await;
    match &create_result {
        Ok(_) => info!("Table '{}' creation request successful", table_name),
        Err(e) => error!("Error creating table '{}': {}", table_name, e),
    }
    create_result?;

    // Wait for the table (and GSIs) to become ACTIVE before running tests
    info!("Waiting for table '{}' to become ACTIVE...", table_name);
    loop {
        match client.describe_table().table_name(table_name).send().await {
            Ok(resp) => {
                if let Some(table_desc) = resp.table() {
                    if table_desc.table_status() == Some(&TableStatus::Active) {
                        // ensure all global secondary indexes are active
                        let gsi_descs = table_desc.global_secondary_indexes();
                        if gsi_descs.is_empty()
                            || gsi_descs
                                .iter()
                                .all(|idx| idx.index_status() == Some(&IndexStatus::Active))
                        {
                            info!("Table '{}' and all GSIs are now ACTIVE!", table_name);
                            break;
                        } else {
                            debug!("Table '{}' is ACTIVE but waiting for GSIs...", table_name);
                        }
                    } else {
                        debug!(
                            "Table '{}' status: {:?}",
                            table_name,
                            table_desc.table_status()
                        );
                    }
                }
            }
            Err(e) => error!("Error checking table status: {}", e),
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    info!("Table '{}' is ready for testing!", table_name);
    Ok(())
}
