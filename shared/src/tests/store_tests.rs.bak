#[cfg(test)]
mod dynamo_tests {
    use crate::models::BoxRecord;
    use crate::models::GuardianStatus;
    use crate::store::{dynamo::DynamoBoxStore, BoxStore};
    use crate::test_utils::test_logging::init_test_logging;
    use aws_sdk_dynamodb::Client;
    use log::info;
    use uuid::Uuid;

    fn create_test_box(name: &str, owner_id: &str) -> BoxRecord {
        let now = crate::models::now_str();
        BoxRecord {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            description: "Test box description".to_string(),
            is_locked: false,
            created_at: now.clone(),
            updated_at: now,
            owner_id: owner_id.to_string(),
            owner_name: Some("Test Owner".to_string()),
            documents: vec![],
            guardians: vec![],
            unlock_instructions: None,
            unlock_request: None,
            version: 0,
        }
    }

    // Helper function to create a DynamoDB client for local testing
    async fn create_local_dynamo_client() -> Client {
        let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .endpoint_url("http://localhost:8000")
            .region("us-east-1")
            .load()
            .await;

        Client::new(&config)
    }

    // Helper function to create a test table for DynamoDB
    async fn create_test_table(
        client: &Client,
        table_name: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use aws_sdk_dynamodb::types::{
            AttributeDefinition, GlobalSecondaryIndex, KeySchemaElement, KeyType, Projection,
            ProjectionType, ScalarAttributeType,
        };

        // First check if table already exists (in case a previous test didn't clean up)
        let existing_tables = client.list_tables().send().await?;
        let table_names = existing_tables.table_names();
        if table_names.contains(&table_name.to_string()) {
            // Table exists, let's delete it first
            client.delete_table().table_name(table_name).send().await?;

            // Wait for the table to be deleted
            let mut is_deleted = false;
            while !is_deleted {
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                let tables = client.list_tables().send().await?;
                let names = tables.table_names();
                is_deleted = !names.contains(&table_name.to_string());
            }
        }

        // Create the test table with GSIs
        // ID attribute (primary key)
        let id_attr = AttributeDefinition::builder()
            .attribute_name("id")
            .attribute_type(ScalarAttributeType::S)
            .build()?;

        // Owner ID attribute (for GSI)
        let owner_id_attr = AttributeDefinition::builder()
            .attribute_name("ownerId")
            .attribute_type(ScalarAttributeType::S)
            .build()?;

        // Primary key schema
        let id_key = KeySchemaElement::builder()
            .attribute_name("id")
            .key_type(KeyType::Hash)
            .build()?;

        // GSI key schema for owner_id
        let owner_id_key = KeySchemaElement::builder()
            .attribute_name("ownerId")
            .key_type(KeyType::Hash)
            .build()?;

        // Create owner_id GSI
        let owner_id_gsi = GlobalSecondaryIndex::builder()
            .index_name("owner_id-index")
            .key_schema(owner_id_key)
            .projection(
                Projection::builder()
                    .projection_type(ProjectionType::All)
                    .build(),
            )
            .provisioned_throughput(
                aws_sdk_dynamodb::types::ProvisionedThroughput::builder()
                    .read_capacity_units(1)
                    .write_capacity_units(1)
                    .build()?,
            )
            .build()?;

        // Now create the table with GSI
        client
            .create_table()
            .table_name(table_name)
            .attribute_definitions(id_attr.clone())
            .attribute_definitions(owner_id_attr)
            .key_schema(id_key)
            .global_secondary_indexes(owner_id_gsi)
            .provisioned_throughput(
                aws_sdk_dynamodb::types::ProvisionedThroughput::builder()
                    .read_capacity_units(1)
                    .write_capacity_units(1)
                    .build()?,
            )
            .send()
            .await?;

        // Wait for the table to become active
        let mut is_active = false;
        while !is_active {
            let describe_response = client
                .describe_table()
                .table_name(table_name)
                .send()
                .await?;

            if let Some(table) = describe_response.table() {
                if table.table_status() == Some(&aws_sdk_dynamodb::types::TableStatus::Active) {
                    is_active = true;
                }
            }

            if !is_active {
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }
        }

        Ok(())
    }

    // Helper function to delete a test table
    async fn delete_test_table(
        client: &Client,
        table_name: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        client.delete_table().table_name(table_name).send().await?;

        // Wait for the table to be deleted
        let mut is_deleted = false;
        while !is_deleted {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

            let tables = client.list_tables().send().await?;
            let names = tables.table_names();
            is_deleted = !names.contains(&table_name.to_string());
        }

        Ok(())
    }

    // Helper function to create a DynamoBoxStore for testing
    async fn create_test_store() -> (DynamoBoxStore, Client, String) {
        let client = create_local_dynamo_client().await;
        let table_name = format!("test-table-{}", Uuid::new_v4());

        // Create the test table
        create_test_table(&client, &table_name)
            .await
            .expect("Failed to create test table");

        // Create the store with our client and table
        let store = DynamoBoxStore::with_client_and_table(client.clone(), table_name.clone());

        (store, client, table_name)
    }

    // Helper function to check if DynamoDB Local is running
    fn is_dynamodb_local_running() -> bool {
        use std::net::TcpStream;
        TcpStream::connect("127.0.0.1:8000").is_ok()
    }

    // Test for creating a box
    #[tokio::test]
    async fn dynamo_store_create_box() {
        init_test_logging();
        // Check if DynamoDB local is running
        if !is_dynamodb_local_running() {
            info!("Skipping test dynamo_store_create_box: DynamoDB Local is not running");
            return;
        }

        // Create the test store
        let (store, client, table_name) = create_test_store().await;

        // Create a test box
        let test_box = create_test_box("DynamoDB Test Box", "test_owner");

        // Test creation
        let result = store.create_box(test_box.clone()).await;
        assert!(result.is_ok());

        let created_box = result.unwrap();
        assert_eq!(created_box.id, test_box.id);
        assert_eq!(created_box.name, "DynamoDB Test Box");

        // Clean up
        delete_test_table(&client, &table_name)
            .await
            .expect("Failed to delete test table");
    }

    // Test for getting a box by ID
    #[tokio::test]
    async fn dynamo_store_get_box() {
        init_test_logging();
        // Check if DynamoDB local is running
        if !is_dynamodb_local_running() {
            info!("Skipping test dynamo_store_get_box: DynamoDB Local is not running");
            return;
        }

        // Create the test store
        let (store, client, table_name) = create_test_store().await;

        // Create a test box
        let test_box = create_test_box("DynamoDB Test Box", "test_owner");
        store.create_box(test_box.clone()).await.unwrap();

        // Test retrieval
        let result = store.get_box(&test_box.id).await;
        assert!(result.is_ok());

        let fetched_box = result.unwrap();
        assert_eq!(fetched_box.id, test_box.id);
        assert_eq!(fetched_box.name, "DynamoDB Test Box");

        // Clean up
        delete_test_table(&client, &table_name)
            .await
            .expect("Failed to delete test table");
    }

    // Test for getting boxes by owner
    #[tokio::test]
    async fn dynamo_store_get_boxes_by_owner() {
        init_test_logging();
        // Check if DynamoDB local is running
        if !is_dynamodb_local_running() {
            info!("Skipping test dynamo_store_get_boxes_by_owner: DynamoDB Local is not running");
            return;
        }

        // Create the test store
        let (store, client, table_name) = create_test_store().await;

        // Create multiple test boxes for the same owner
        let owner_id = "test_owner";
        let test_box1 = create_test_box("Box 1", owner_id);
        let test_box2 = create_test_box("Box 2", owner_id);
        let test_box3 = create_test_box("Box 3", "different_owner");

        store.create_box(test_box1.clone()).await.unwrap();
        store.create_box(test_box2.clone()).await.unwrap();
        store.create_box(test_box3.clone()).await.unwrap();

        // Test retrieval by owner
        let result = store.get_boxes_by_owner(owner_id).await;
        assert!(result.is_ok());

        let fetched_boxes = result.unwrap();
        assert_eq!(fetched_boxes.len(), 2); // Should only get the two boxes for test_owner

        // Verify the boxes belong to our owner
        for box_rec in &fetched_boxes {
            assert_eq!(box_rec.owner_id, owner_id);
        }

        // Clean up
        delete_test_table(&client, &table_name)
            .await
            .expect("Failed to delete test table");
    }

    // Test for updating a box
    #[tokio::test]
    async fn dynamo_store_update_box() {
        init_test_logging();
        // Check if DynamoDB local is running
        if !is_dynamodb_local_running() {
            info!("Skipping test dynamo_store_update_box: DynamoDB Local is not running");
            return;
        }

        // Create the test store
        let (store, client, table_name) = create_test_store().await;

        // Create a test box
        let test_box = create_test_box("Original Name", "test_owner");
        store.create_box(test_box.clone()).await.unwrap();

        // Update the box with a new name
        let mut updated_box = test_box.clone();
        updated_box.name = "Updated Name".to_string();

        let result = store.update_box(updated_box.clone()).await;
        assert!(result.is_ok());

        // Verify the box was updated
        let fetched_box = store.get_box(&test_box.id).await.unwrap();
        assert_eq!(fetched_box.name, "Updated Name");

        // Clean up
        delete_test_table(&client, &table_name)
            .await
            .expect("Failed to delete test table");
    }

    // Test for deleting a box
    #[tokio::test]
    async fn dynamo_store_delete_box() {
        init_test_logging();
        // Check if DynamoDB local is running
        if !is_dynamodb_local_running() {
            info!("Skipping test dynamo_store_delete_box: DynamoDB Local is not running");
            return;
        }

        // Create the test store
        let (store, client, table_name) = create_test_store().await;

        // Create a test box
        let test_box = create_test_box("Box to Delete", "test_owner");
        store.create_box(test_box.clone()).await.unwrap();

        // Delete the box
        let result = store.delete_box(&test_box.id).await;
        assert!(result.is_ok());

        // Verify the box was deleted
        let get_result = store.get_box(&test_box.id).await;
        assert!(get_result.is_err());

        // Clean up
        delete_test_table(&client, &table_name)
            .await
            .expect("Failed to delete test table");
    }

    // Test for getting boxes by guardian ID
    #[tokio::test]
    async fn dynamo_store_get_boxes_by_guardian_id() {
        init_test_logging();
        // Check if DynamoDB local is running
        if !is_dynamodb_local_running() {
            info!("Skipping test dynamo_store_get_boxes_by_guardian_id: DynamoDB Local is not running");
            return;
        }

        // Create the test store
        let (store, client, table_name) = create_test_store().await;

        // Create test boxes with guardians
        let guardian_id = "guardian_id";

        // Box 1 - has test_guardian as a guardian
        let mut test_box1 = create_test_box("Box with Guardian", "test_owner");
        test_box1.guardians.push(crate::models::Guardian {
            id: guardian_id.to_string(),
            name: "Test Guardian".to_string(),
            status: GuardianStatus::Accepted,
            lead_guardian: false,
            added_at: crate::models::now_str(),
            invitation_id: Uuid::new_v4().to_string(),
        });

        // Box 2 - has test_guardian as a rejected guardian (shouldn't show up)
        let mut test_box2 = create_test_box("Box with Rejected Guardian", "test_owner");
        test_box2.guardians.push(crate::models::Guardian {
            id: guardian_id.to_string(),
            name: "Test Guardian".to_string(),
            status: GuardianStatus::Rejected,
            lead_guardian: false,
            added_at: crate::models::now_str(),
            invitation_id: Uuid::new_v4().to_string(),
        });

        // Box 3 - different guardian
        let mut test_box3 = create_test_box("Box with Different Guardian", "test_owner");
        test_box3.guardians.push(crate::models::Guardian {
            id: "other_guardian".to_string(),
            name: "Other Guardian".to_string(),
            status: GuardianStatus::Accepted,
            lead_guardian: false,
            added_at: crate::models::now_str(),
            invitation_id: Uuid::new_v4().to_string(),
        });

        store.create_box(test_box1.clone()).await.unwrap();
        store.create_box(test_box2.clone()).await.unwrap();
        store.create_box(test_box3.clone()).await.unwrap();

        // Test retrieval by guardian ID
        let result = store.get_boxes_by_guardian_id(guardian_id).await;
        assert!(result.is_ok());

        let fetched_boxes = result.unwrap();
        // Should only get Box 1 (with accepted guardian status)
        assert_eq!(fetched_boxes.len(), 1);
        assert_eq!(fetched_boxes[0].id, test_box1.id);

        // Clean up
        delete_test_table(&client, &table_name)
            .await
            .expect("Failed to delete test table");
    }
}
