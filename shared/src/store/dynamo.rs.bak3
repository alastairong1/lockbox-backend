use async_trait::async_trait;
use aws_config::BehaviorVersion;
use aws_sdk_dynamodb::error::SdkError;
use aws_sdk_dynamodb::operation::delete_item::DeleteItemError;
use aws_sdk_dynamodb::operation::get_item::GetItemError;
use aws_sdk_dynamodb::operation::query::QueryError;
use aws_sdk_dynamodb::operation::scan::ScanError;
use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::Client;
use chrono::{Duration, Utc};
use serde_dynamo::{from_item, to_item};
use std::collections::HashMap;
use std::env;

use crate::error::{map_dynamo_error, Result, StoreError};
use crate::models::{now_str, BoxRecord, Invitation};

// Invitation Store Constants
const TABLE_NAME: &str = "invitation-table";
const GSI_BOX_ID: &str = "box_id-index";
const GSI_INVITE_CODE: &str = "invite_code-index";
const GSI_CREATOR_ID: &str = "creatorId-index";

// Box Store Constants
const BOX_TABLE_NAME: &str = "box-table";
const GSI_OWNER_ID: &str = "owner_id-index";

// DynamoInvitationStore

pub struct DynamoInvitationStore {
    client: Client,
    table_name: String,
}

impl DynamoInvitationStore {
    pub async fn new() -> Self {
        // Use the recommended defaults() function with latest behavior version
        let config = aws_config::defaults(BehaviorVersion::latest()).load().await;

        let client = Client::new(&config);

        // Use environment variable for table name if available
        let table_name =
            env::var("DYNAMODB_INVITATION_TABLE").unwrap_or_else(|_| TABLE_NAME.to_string());

        Self { client, table_name }
    }

    /// Creates a new DynamoDB store with the specified client and table name.
    /// This is mainly useful for testing with a local DynamoDB instance.
    #[allow(dead_code)]
    pub fn with_client_and_table(client: Client, table_name: String) -> Self {
        Self { client, table_name }
    }

    /// Helper method to check if an invitation has expired
    fn is_expired(&self, expires_at_str: &str) -> Result<bool> {
        let expires_at = chrono::DateTime::parse_from_rfc3339(expires_at_str)
            .map_err(|_| StoreError::InternalError("Invalid expiration date format".to_string()))?
            .with_timezone(&Utc);

        Ok(Utc::now() > expires_at)
    }
}

// DynamoBoxStore

/// DynamoDB store for boxes
pub struct DynamoBoxStore {
    client: Client,
    table_name: String,
}

impl DynamoBoxStore {
    /// Creates a new DynamoDB store
    pub async fn new() -> Self {
        // Use the recommended defaults() function with latest behavior version
        let config = aws_config::defaults(BehaviorVersion::latest()).load().await;

        let client = Client::new(&config);

        // Use environment variable for table name if available
        let table_name = env::var("DYNAMODB_TABLE").unwrap_or_else(|_| BOX_TABLE_NAME.to_string());

        Self { client, table_name }
    }

    /// Creates a new DynamoDB store with the specified client and table name.
    /// This is mainly useful for testing with a local DynamoDB instance.
    #[allow(dead_code)]
    pub fn with_client_and_table(client: Client, table_name: String) -> Self {
        Self { client, table_name }
    }
}

#[async_trait]
impl super::BoxStore for DynamoBoxStore {
    /// Creates a new box record in DynamoDB
    async fn create_box(&self, box_record: BoxRecord) -> Result<BoxRecord> {
        let item = to_item(&box_record)?;

        self.client
            .put_item()
            .table_name(&self.table_name)
            .set_item(Some(item))
            .send()
            .await
            .map_err(|e| map_dynamo_error("put_item", e))?;

        Ok(box_record)
    }

    /// Gets a box by ID
    async fn get_box(&self, id: &str) -> Result<BoxRecord> {
        let key = HashMap::from([("id".to_string(), AttributeValue::S(id.to_string()))]);

        let response = self
            .client
            .get_item()
            .table_name(&self.table_name)
            .set_key(Some(key))
            .send()
            .await
            .map_err(|e| map_get_dynamo_error(e, id))?;

        let item = response
            .item()
            .ok_or_else(|| StoreError::NotFound(format!("Box not found: {}", id)))?;

        let box_record = from_item(item.clone())?;
        Ok(box_record)
    }

    /// Gets all boxes owned by a user
    async fn get_boxes_by_owner(&self, owner_id: &str) -> Result<Vec<BoxRecord>> {
        let expr_attr_names = HashMap::from([("#owner_id".to_string(), "ownerId".to_string())]);

        let expr_attr_values = HashMap::from([(
            ":owner_id".to_string(),
            AttributeValue::S(owner_id.to_string()),
        )]);

        let response = self
            .client
            .query()
            .table_name(&self.table_name)
            .index_name(GSI_OWNER_ID) // Use the GSI
            .key_condition_expression("#owner_id = :owner_id")
            .set_expression_attribute_names(Some(expr_attr_names))
            .set_expression_attribute_values(Some(expr_attr_values))
            .send()
            .await
            .map_err(|e| map_query_dynamo_error(e))?;

        // items() returns a reference to a slice, which could be empty but not None
        let items = response.items();

        let mut boxes = Vec::new();
        for item in items {
            let box_record = from_item(item.clone())?;
            boxes.push(box_record);
        }

        Ok(boxes)
    }

    /// Updates a box
    async fn update_box(&self, box_record: BoxRecord) -> Result<BoxRecord> {
        // Clone the box record for modification
        let mut updated_box = box_record.clone();

        // Update the timestamp
        updated_box.updated_at = now_str();

        // Increment the version number
        let current_version = updated_box.version;
        updated_box.version = current_version + 1;

        // Convert to DynamoDB item
        let item = to_item(&updated_box)?;

        // Create a conditional expression to check the version
        let condition_expression = if current_version > 0 {
            "#v = :current_version"
        } else {
            "attribute_not_exists(#v) OR #v = :current_version"
        };

        // Create expression attribute values for version check
        let mut expr_attr_values = HashMap::new();
        expr_attr_values.insert(
            ":current_version".to_string(),
            AttributeValue::N(current_version.to_string()),
        );

        // Create expression attribute names to handle reserved keywords
        let expr_attr_names = HashMap::from([("#v".to_string(), "version".to_string())]);

        // Build the update request with conditional expression
        let request = self
            .client
            .put_item()
            .table_name(&self.table_name)
            .set_item(Some(item))
            .condition_expression(condition_expression)
            .set_expression_attribute_values(Some(expr_attr_values))
            .set_expression_attribute_names(Some(expr_attr_names));

        // Execute the update
        match request.send().await {
            Ok(_) => Ok(updated_box),
            Err(err) => {
                // Check if it's a conditional check failure (version mismatch)
                if let SdkError::ServiceError(service_err) = &err {
                    if service_err.err().is_conditional_check_failed_exception() {
                        // Version conflict - retry with the latest version
                        return Err(StoreError::VersionConflict(format!(
                            "Box update conflict: id={}, version={}",
                            updated_box.id, current_version
                        )));
                    }
                }

                // Other error
                Err(map_dynamo_error("put_item", err))
            }
        }
    }

    /// Deletes a box
    async fn delete_box(&self, id: &str) -> Result<()> {
        let key = HashMap::from([("id".to_string(), AttributeValue::S(id.to_string()))]);

        self.client
            .delete_item()
            .table_name(&self.table_name)
            .set_key(Some(key))
            .send()
            .await
            .map_err(|e| map_delete_dynamo_error(e))?;

        Ok(())
    }

    /// Gets all boxes where the given user is a guardian (with status not rejected)
    ///
    /// Implementation notes:
    /// - Currently uses a full table scan since guardians are stored in nested arrays within the BoxRecord
    /// - For production systems with many boxes, this could be improved by:
    ///   1. Creating a new GSI with a composite key or
    ///   2. Creating a separate guardian-to-box mapping table with a GSI
    ///   3. Using DynamoDB's new document path capabilities for filtering
    async fn get_boxes_by_guardian_id(&self, guardian_id: &str) -> Result<Vec<BoxRecord>> {
        // Currently we perform a full table scan as guardian information is stored in an array within
        // the box document, not as a separate attribute that can be indexed. In the future, we could
        // create a separate table or GSI for guardian relationships.

        let response = self
            .client
            .scan()
            .table_name(&self.table_name)
            .send()
            .await
            .map_err(|e| map_scan_dynamo_error(e))?;

        let items = response.items();

        let mut boxes = Vec::new();
        for item in items {
            let box_record: BoxRecord = from_item(item.clone())?;

            // Check if the user is a guardian for this box
            let is_guardian = box_record.guardians.iter().any(|guardian| {
                guardian.id == guardian_id
                    && guardian.status != crate::models::GuardianStatus::Rejected
            });

            if is_guardian {
                boxes.push(box_record);
            }
        }

        Ok(boxes)
    }
}

// INVITATION STORE IMPLEMENTATION
#[async_trait]
impl super::InvitationStore for DynamoInvitationStore {
    async fn create_invitation(&self, mut invitation: Invitation) -> Result<Invitation> {
        // Set created_at and expires_at if not already set
        if invitation.created_at.is_empty() {
            invitation.created_at = Utc::now().to_rfc3339();
        }

        if invitation.expires_at.is_empty() {
            // Set expiration to 48 hours from now
            invitation.expires_at = (Utc::now() + Duration::hours(48)).to_rfc3339();
        }

        log::debug!(
            "Storing invitation id={}, creator_id={}, invite_code={}, to table={}",
            invitation.id,
            invitation.creator_id,
            invitation.invite_code,
            self.table_name
        );

        // Convert to DynamoDB item
        let item = to_item(invitation.clone())?;

        self.client
            .put_item()
            .table_name(&self.table_name)
            .set_item(Some(item))
            .send()
            .await
            .map_err(|e| map_dynamo_error("put_item", e))?;

        log::debug!("Successfully stored invitation {}", invitation.id);
        Ok(invitation)
    }

    async fn get_invitation(&self, id: &str) -> Result<Invitation> {
        let key = HashMap::from([("id".to_string(), AttributeValue::S(id.to_string()))]);

        let result = self
            .client
            .get_item()
            .table_name(&self.table_name)
            .set_key(Some(key))
            .send()
            .await
            .map_err(|e| map_dynamo_error("get_item", e))?;

        let item = result
            .item()
            .ok_or_else(|| StoreError::NotFound(format!("Invitation with id {} not found", id)))?;

        let invitation: Invitation = from_item(item.clone())?;

        // Check if the invitation has expired
        if self.is_expired(&invitation.expires_at)? {
            return Err(StoreError::InvitationExpired);
        }

        Ok(invitation)
    }

    async fn get_invitation_by_code(&self, invite_code: &str) -> Result<Invitation> {
        // Create expression attribute values
        let expr_attr_values = HashMap::from([(
            ":invite_code".to_string(),
            AttributeValue::S(invite_code.to_string()),
        )]);

        let result = self
            .client
            .query()
            .table_name(&self.table_name)
            .index_name(GSI_INVITE_CODE)
            .key_condition_expression("inviteCode = :invite_code")
            .set_expression_attribute_values(Some(expr_attr_values))
            .send()
            .await
            .map_err(|e| map_dynamo_error("query", e))?;

        let items = result.items();

        if items.is_empty() {
            return Err(StoreError::NotFound(format!(
                "Invitation with code {} not found",
                invite_code
            )));
        }

        let invitation: Invitation = from_item(items[0].clone())?;

        // Check if the invitation has expired
        if self.is_expired(&invitation.expires_at)? {
            return Err(StoreError::InvitationExpired);
        }

        Ok(invitation)
    }

    async fn update_invitation(&self, invitation: Invitation) -> Result<Invitation> {
        // VULNERABILITY: Lacks optimistic concurrency control (OCC)
        // This method unconditionally overwrites the item in DynamoDB,
        // which could lead to silent lost-update races in concurrent scenarios.

        // RECOMMENDATION: Implement version-based OCC similar to update_box:
        // 1. Add a version field to Invitation model (default=0, skip_serializing)
        // 2. Clone invitation and increment its version
        // 3. Create a conditional expression to check current version
        // 4. Handle ConditionalCheckFailedException as StoreError::VersionConflict

        // Convert to DynamoDB item
        let item = to_item(invitation.clone())?;

        self.client
            .put_item()
            .table_name(&self.table_name)
            .set_item(Some(item))
            .send()
            .await
            .map_err(|e| map_dynamo_error("put_item", e))?;

        Ok(invitation)
    }

    async fn delete_invitation(&self, id: &str) -> Result<()> {
        // Verify invitation exists first
        self.get_invitation(id).await?;

        let key = HashMap::from([("id".to_string(), AttributeValue::S(id.to_string()))]);

        self.client
            .delete_item()
            .table_name(&self.table_name)
            .set_key(Some(key))
            .send()
            .await
            .map_err(|e| map_dynamo_error("delete_item", e))?;

        Ok(())
    }

    async fn get_invitations_by_box_id(&self, box_id: &str) -> Result<Vec<Invitation>> {
        // Create expression attribute values
        let expr_attr_values =
            HashMap::from([(":box_id".to_string(), AttributeValue::S(box_id.to_string()))]);

        let result = self
            .client
            .query()
            .table_name(&self.table_name)
            .index_name(GSI_BOX_ID)
            .key_condition_expression("boxId = :box_id")
            .set_expression_attribute_values(Some(expr_attr_values))
            .send()
            .await
            .map_err(|e| map_dynamo_error("query", e))?;

        let items = result.items();

        let mut invitations = Vec::new();
        for item in items {
            let invitation: Invitation = from_item(item.clone())?;
            // Filter out expired invitations
            if !self.is_expired(&invitation.expires_at)? {
                invitations.push(invitation);
            }
        }

        Ok(invitations)
    }

    async fn get_invitations_by_creator_id(&self, creator_id: &str) -> Result<Vec<Invitation>> {
        log::info!(
            "Querying table {} for invitations with creator_id={}, using GSI: {}",
            self.table_name,
            creator_id,
            GSI_CREATOR_ID
        );

        // Create expression attribute values
        let expr_attr_values = HashMap::from([(
            ":creatorId".to_string(),
            AttributeValue::S(creator_id.to_string()),
        )]);

        log::info!(
            "Query parameters: table={}, index={}, expression=creatorId = :creatorId, values={:?}",
            self.table_name,
            GSI_CREATOR_ID,
            expr_attr_values
        );

        let result = self
            .client
            .query()
            .table_name(&self.table_name)
            .index_name(GSI_CREATOR_ID)
            .key_condition_expression("creatorId = :creatorId")
            .set_expression_attribute_values(Some(expr_attr_values))
            .send()
            .await
            .map_err(|e| map_dynamo_error("query", e))?;

        let items = result.items();
        log::info!("Found {} items for creator_id={}", items.len(), creator_id);

        if items.is_empty() {
            log::info!("No items found. Checking if this is a case sensitivity issue or wrong attribute name...");
            // Let's try to scan a few items to see what creator_id values actually exist
            let scan_result = self
                .client
                .scan()
                .table_name(&self.table_name)
                .limit(5)
                .send()
                .await
                .map_err(|e| map_dynamo_error("scan", e))?;

            let scan_items = scan_result.items();
            log::info!("Sample scan found {} items in table", scan_items.len());
            for (i, item) in scan_items.iter().enumerate() {
                if let Some(creator_attr) = item.get("creatorId") {
                    log::info!("Sample item {}: creatorId = {:?}", i, creator_attr);
                } else {
                    log::info!(
                        "Sample item {}: no creatorId attribute found, available attributes: {:?}",
                        i,
                        item.keys().collect::<Vec<_>>()
                    );
                }
            }
        }

        let mut invitations = Vec::new();
        for item in items {
            let invitation: Invitation = from_item(item.clone())?;
            invitations.push(invitation);
        }
        Ok(invitations)
    }
}

// Helper functions for DynamoDB error mapping
fn map_get_dynamo_error(err: SdkError<GetItemError>, id: &str) -> StoreError {
    match err {
        SdkError::ServiceError(ref service_err) => {
            if let GetItemError::ResourceNotFoundException(_) = service_err.err() {
                StoreError::NotFound(format!("Box not found: {}", id))
            } else {
                StoreError::InternalError(format!("DynamoDB get_item error: {}", err))
            }
        }
        _ => StoreError::InternalError(format!("DynamoDB get_item error: {}", err)),
    }
}

fn map_delete_dynamo_error(err: SdkError<DeleteItemError>) -> StoreError {
    StoreError::InternalError(format!("DynamoDB delete_item error: {}", err))
}

fn map_query_dynamo_error(err: SdkError<QueryError>) -> StoreError {
    StoreError::InternalError(format!("DynamoDB query error: {}", err))
}

fn map_scan_dynamo_error(err: SdkError<ScanError>) -> StoreError {
    StoreError::InternalError(format!("DynamoDB scan error: {}", err))
}

// Builder pattern alternative
impl DynamoInvitationStore {
    /// Creates a new builder to configure a DynamoInvitationStore
    pub fn builder() -> DynamoInvitationStoreBuilder {
        DynamoInvitationStoreBuilder::default()
    }

    /// Synchronous constructor that creates a store with default AWS config
    /// Note: This should only be used in non-async contexts
    pub fn new_blocking() -> Self {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create Tokio runtime");

        runtime.block_on(Self::new())
    }
}

pub struct DynamoInvitationStoreBuilder {
    table_name: Option<String>,
}

impl Default for DynamoInvitationStoreBuilder {
    fn default() -> Self {
        Self { table_name: None }
    }
}

impl DynamoInvitationStoreBuilder {
    pub fn table_name(mut self, table_name: String) -> Self {
        self.table_name = Some(table_name);
        self
    }

    pub async fn build(self) -> DynamoInvitationStore {
        let config = aws_config::defaults(BehaviorVersion::latest()).load().await;
        let client = Client::new(&config);

        let table_name = self.table_name.unwrap_or_else(|| {
            env::var("DYNAMODB_INVITATION_TABLE").unwrap_or_else(|_| TABLE_NAME.to_string())
        });

        DynamoInvitationStore { client, table_name }
    }
}
