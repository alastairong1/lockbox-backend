use serde::{Deserialize, Serialize};

// Import shared models for direct use in request/response types
use lockbox_shared::models::{Document, Guardian, UnlockRequest};

// Request DTOs
#[derive(Deserialize, Debug)]
pub struct CreateBoxRequest {
    pub name: String,
    pub description: String,
    #[serde(rename = "ownerName", skip_serializing_if = "Option::is_none")]
    pub owner_name: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct UpdateBoxRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(
        rename = "unlockInstructions",
        skip_serializing_if = "Option::is_none",
        default,
        with = "optional_field_serde"
    )]
    pub unlock_instructions: Option<OptionalField<String>>,
    #[serde(rename = "isLocked", skip_serializing_if = "Option::is_none")]
    pub is_locked: Option<bool>,
}

#[derive(Deserialize, Debug)]
pub struct DocumentUpdateRequest {
    pub document: Document,
}

#[derive(Deserialize, Debug)]
pub struct GuardianUpdateRequest {
    pub guardian: Guardian,
}

#[derive(Deserialize, Debug)]
pub struct IncomingShard {
    #[serde(rename = "guardianId")]
    pub guardian_id: String,
    pub shard: String,
    #[serde(rename = "shardHash")]
    pub shard_hash: String,
}

#[derive(Deserialize, Debug)]
pub struct LockBoxRequest {
    #[serde(rename = "shardThreshold")]
    pub shard_threshold: usize,
    pub shards: Vec<IncomingShard>,
}

// Response DTOs
#[derive(Serialize, Debug)]
pub struct BoxResponse {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    #[serde(rename = "unlockInstructions")]
    pub unlock_instructions: Option<String>,
    #[serde(rename = "isLocked")]
    pub is_locked: bool,
    #[serde(rename = "lockedAt", skip_serializing_if = "Option::is_none")]
    pub locked_at: Option<String>,
    pub documents: Vec<Document>,
    pub guardians: Vec<Guardian>,
    #[serde(rename = "ownerId")]
    pub owner_id: String,
    #[serde(rename = "ownerName")]
    pub owner_name: Option<String>,
    #[serde(rename = "unlockRequest")]
    pub unlock_request: Option<UnlockRequest>,
    #[serde(rename = "shardThreshold", skip_serializing_if = "Option::is_none")]
    pub shard_threshold: Option<u32>,
    #[serde(rename = "shardsFetched", skip_serializing_if = "Option::is_none")]
    pub shards_fetched: Option<usize>,
    #[serde(rename = "totalShards", skip_serializing_if = "Option::is_none")]
    pub total_shards: Option<usize>,
    #[serde(rename = "shardsDeletedAt", skip_serializing_if = "Option::is_none")]
    pub shards_deleted_at: Option<String>,
}

impl From<lockbox_shared::models::BoxRecord> for BoxResponse {
    fn from(box_rec: lockbox_shared::models::BoxRecord) -> Self {
        Self {
            id: box_rec.id,
            name: box_rec.name,
            description: box_rec.description,
            created_at: box_rec.created_at,
            updated_at: box_rec.updated_at,
            unlock_instructions: box_rec.unlock_instructions,
            is_locked: box_rec.is_locked,
            locked_at: box_rec.locked_at,
            documents: box_rec.documents,
            guardians: box_rec.guardians,
            owner_id: box_rec.owner_id,
            owner_name: box_rec.owner_name,
            unlock_request: box_rec.unlock_request,
            shard_threshold: box_rec.shard_threshold,
            shards_fetched: box_rec.shards_fetched,
            total_shards: box_rec.total_shards,
            shards_deleted_at: box_rec.shards_deleted_at,
        }
    }
}

#[derive(Serialize, Debug)]
pub struct DocumentUpdateResponse {
    pub documents: Vec<Document>,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

#[derive(Serialize, Debug)]
pub struct GuardianUpdateResponse {
    pub id: String,
    pub name: String,
    pub status: String,
    #[serde(rename = "leadGuardian")]
    pub lead_guardian: bool,
    #[serde(rename = "addedAt")]
    pub added_at: String,
    #[serde(rename = "invitationId")]
    pub invitation_id: String,
    #[serde(rename = "allGuardians")]
    pub all_guardians: Vec<Guardian>,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

// Helper for null vs. not-present in JSON
// Custom wrapper type to differentiate between field not present and field present but null
#[derive(Debug)]
pub enum OptionalField<T> {
    Value(T),
    Null,
}

// Custom serde module for optional fields that need to distinguish between null and absent
mod optional_field_serde {
    use super::OptionalField;
    use serde::{Deserialize, Deserializer};

    pub fn deserialize<'de, D, T>(deserializer: D) -> Result<Option<OptionalField<T>>, D::Error>
    where
        D: Deserializer<'de>,
        T: Deserialize<'de>,
    {
        let option = Option::<T>::deserialize(deserializer)?;
        match option {
            Some(val) => Ok(Some(OptionalField::Value(val))),
            None => Ok(Some(OptionalField::Null)), // null was explicitly provided
        }
    }

    // pub fn serialize<S, T>(
    //     value: &Option<OptionalField<T>>,
    //     serializer: S,
    // ) -> Result<S::Ok, S::Error>
    // where
    //     S: Serializer,
    //     T: serde::Serialize,
    // {
    //     match value {
    //         Some(OptionalField::Value(val)) => serializer.serialize_some(val),
    //         Some(OptionalField::Null) => serializer.serialize_none(),
    //         None => serializer.serialize_none(),
    //     }
    // }
}

// Additional request/response types
#[derive(Deserialize, Debug)]
pub struct LeadGuardianUpdateRequest {
    pub message: String,
}

#[derive(Deserialize, Debug)]
pub struct GuardianResponseRequest {
    pub approve: Option<bool>,
    pub reject: Option<bool>,
}

#[derive(Deserialize, Debug)]
pub struct GuardianInvitationResponse {
    pub accept: bool,
}

// Use shared ErrorResponse and MessageResponse from lockbox_shared

// GuardianBox DTO to exclude version
#[derive(Serialize, Debug)]
pub struct GuardianBoxResponse {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(rename = "isLocked")]
    pub is_locked: bool,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    #[serde(rename = "ownerId")]
    pub owner_id: String,
    #[serde(rename = "ownerName")]
    pub owner_name: Option<String>,
    #[serde(rename = "unlockInstructions")]
    pub unlock_instructions: Option<String>,
    #[serde(rename = "unlockRequest")]
    pub unlock_request: Option<UnlockRequest>,
    #[serde(rename = "pendingGuardianApproval")]
    pub pending_guardian_approval: Option<bool>,
    #[serde(rename = "guardiansCount")]
    pub guardians_count: usize,
    #[serde(rename = "isLeadGuardian")]
    pub is_lead_guardian: bool,
    pub documents: Vec<Document>,
    pub guardians: Vec<Guardian>,
}

impl From<lockbox_shared::models::GuardianBox> for GuardianBoxResponse {
    fn from(guard_box: lockbox_shared::models::GuardianBox) -> Self {
        Self {
            id: guard_box.id,
            name: guard_box.name,
            description: guard_box.description,
            is_locked: guard_box.is_locked,
            created_at: guard_box.created_at,
            updated_at: guard_box.updated_at,
            owner_id: guard_box.owner_id,
            owner_name: guard_box.owner_name,
            unlock_instructions: guard_box.unlock_instructions,
            unlock_request: guard_box.unlock_request,
            pending_guardian_approval: guard_box.pending_guardian_approval,
            guardians_count: guard_box.guardians_count,
            is_lead_guardian: guard_box.is_lead_guardian,
            documents: guard_box.documents,
            guardians: guard_box.guardians,
        }
    }
}

// Use shared now_str()
