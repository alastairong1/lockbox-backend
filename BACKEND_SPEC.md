# Lockbox Backend Specification

Derived from the test suite covering box-service, invitation-service, invitation-event-service, and shared libraries.

---

## Overview

Lockbox is a secure document storage backend built as a collection of serverless microservices on AWS. Box owners store encrypted documents in "boxes" and assign guardians who can collectively unlock boxes in emergency scenarios using a shard-based secret sharing scheme.

**Tech stack:** Rust, Axum, AWS Lambda, DynamoDB, SNS, Cognito (JWT auth)

---

## Authentication

- All API endpoints require a valid JWT bearer token (Cognito).
- Requests without an `Authorization` header return **401 Unauthorized**.
- User identity is extracted from the token and used for authorization checks on every resource operation.

---

## Data Models

### BoxRecord

| Field | Type | Description |
|---|---|---|
| id | string | Unique box identifier |
| name | string | Display name |
| description | string | Box description |
| isLocked | bool | Whether the box is locked (immutable once true) |
| lockedAt | string? | ISO-8601 timestamp of when the box was locked |
| createdAt | string | ISO-8601 creation timestamp |
| updatedAt | string | ISO-8601 last update timestamp |
| ownerId | string | Cognito user ID of the box owner |
| ownerName | string? | Display name of the owner |
| documents | Document[] | Encrypted documents stored in the box |
| guardians | Guardian[] | List of guardians assigned to the box |
| unlockInstructions | string? | Instructions for guardians on how to unlock |
| unlockRequest | UnlockRequest? | Active unlock request, if any |
| version | u64 | Optimistic concurrency control version |
| shardThreshold | u32? | Minimum number of shards required to unlock |
| totalShards | usize? | Total number of shards distributed |
| shardsFetched | usize? | Number of shards acknowledged by guardians |
| shardsDeletedAt | string? | Timestamp when all shards were fetched and server copies deleted |

### Document

| Field | Type | Description |
|---|---|---|
| id | string | Unique document identifier |
| title | string | Document title |
| encryptedContent | string? | Encrypted document payload |
| createdAt | string | ISO-8601 creation timestamp |

### Guardian

| Field | Type | Description |
|---|---|---|
| id | string | Cognito user ID (empty until invitation is opened) |
| name | string | Display name |
| leadGuardian | bool | Whether this guardian can initiate unlock requests |
| status | GuardianStatus | Current status in the guardian lifecycle |
| addedAt | string | ISO-8601 timestamp |
| invitationId | string | Link to the associated invitation |
| lockDataReceivedAt | string? | When the guardian received lock data |
| encryptedShard | string? | Server-side copy of the encrypted shard (deleted after acknowledgment) |
| shardHash | string? | Hash of the shard for verification |
| shardFetchedAt | string? | When the guardian fetched their shard |
| shardAcceptedAt | string? | When the guardian accepted their shard |

**GuardianStatus lifecycle:** `invited` -> `viewed` -> `accepted` | `rejected`

### UnlockRequest

| Field | Type | Description |
|---|---|---|
| id | string | Unique request identifier |
| requestedAt | string | ISO-8601 timestamp |
| status | UnlockRequestStatus | `requested` / `approved` / `rejected` / `completed` |
| message | string? | Message from the initiating guardian |
| initiatedBy | string? | User ID of the lead guardian who initiated |
| approvedBy | string[] | List of guardian user IDs who approved |
| rejectedBy | string[] | List of guardian user IDs who rejected |

### Invitation

| Field | Type | Description |
|---|---|---|
| id | string | Unique invitation identifier |
| inviteCode | string | 8-character uppercase alphabetic code (A-Z) |
| invitedName | string | Name of the person being invited |
| boxId | string | Associated box ID |
| createdAt | string | ISO-8601 timestamp |
| expiresAt | string | ISO-8601 expiry (48 hours from creation/refresh) |
| opened | bool | Whether the invitation has been redeemed |
| linkedUserId | string? | User ID of the person who redeemed the invite |
| creatorId | string | User ID of the invitation creator |
| isLeadGuardian | bool | Whether the invited guardian will be a lead guardian |

---

## API Endpoints

### Box Service -- Owner Endpoints

#### `GET /boxes/owned`
List all boxes owned by the authenticated user.

- **Auth:** Required (owner identity from token)
- **Response:** `200 OK` -- `{ "boxes": BoxRecord[] }`
- Returns only boxes where `ownerId` matches the caller.

#### `GET /boxes/owned/:boxId`
Get a specific box by ID.

- **Auth:** Required
- **Response:**
  - `200 OK` -- `{ "box": BoxRecord }` (fields include `id`, `name`, `description`, `createdAt`, `updatedAt`, `isLocked`, `lockedAt`, `documents`, `guardians`, `ownerId`)
  - `401 Unauthorized` -- if caller is not the box owner
  - `404 Not Found` -- if box does not exist

#### `POST /boxes/owned`
Create a new box.

- **Auth:** Required
- **Body:** `{ "name": string, "description": string }`
- **Response:**
  - `201 Created` -- `{ "box": BoxRecord }`
  - `4xx` -- if payload is invalid (e.g., missing `name`)
- Box is created with `isLocked: false`, empty documents and guardians, and the caller as owner.

#### `PATCH /boxes/owned/:boxId`
Update box metadata. Supports partial updates.

- **Auth:** Required (owner only)
- **Body:** Any subset of `{ "name", "description", "isLocked", "unlockInstructions" }`
- **Response:**
  - `200 OK` -- `{ "box": BoxRecord }`
  - `401 Unauthorized` -- if caller is not the owner
  - `400 Bad Request` -- if the box is locked and the update attempts to change any field (see immutability rules below)
- **Partial update behavior:** Only supplied fields are modified; omitted fields retain their current values.
- **Lock behavior:**
  - Setting `isLocked: true` sets `lockedAt` to current timestamp.
  - Locking an already-locked box is idempotent (returns `200`, `lockedAt` does not change).
  - Setting `isLocked: false` on a locked box returns `400` -- boxes cannot be unlocked via this endpoint.
- **Unlock instructions:** Can be set to a string or cleared by passing `null`.

#### `DELETE /boxes/owned/:boxId`
Delete a box.

- **Auth:** Required (owner only)
- **Response:**
  - `200 OK`
  - `4xx` -- if caller is not the owner
- Locked boxes **can** be deleted by their owner.

---

### Box Service -- Document Endpoints

#### `PATCH /boxes/owned/:boxId/document`
Add or update a document in a box.

- **Auth:** Required (owner only)
- **Body:** `{ "document": { "id", "title", "encryptedContent", "createdAt" } }`
- **Response:**
  - `200 OK` -- `{ "document": { "documents": Document[] } }`
  - `401 Unauthorized` -- if not the owner
  - `422 Unprocessable Entity` -- if payload is invalid (missing `document` field)
  - `400 Bad Request` -- if box is locked
- If a document with the same `id` already exists, it is replaced (upsert behavior).

#### `DELETE /boxes/owned/:boxId/document/:documentId`
Delete a document from a box.

- **Auth:** Required (owner only)
- **Response:**
  - `200 OK` -- `{ "message": string, "document": ... }`
  - `401 Unauthorized` -- if not the owner
  - `404 Not Found` -- if document does not exist
  - `400 Bad Request` -- if box is locked

---

### Box Service -- Guardian Management (Owner)

#### `PATCH /boxes/owned/:boxId/guardian`
Add or update a guardian on a box.

- **Auth:** Required (owner only)
- **Body:** `{ "guardian": { "id", "name", "leadGuardian", "status", "addedAt", "invitationId" } }`
- **Response:**
  - `422 Unprocessable Entity` -- direct guardian add/update via this endpoint is rejected; guardians must be added through the invitation flow
  - `400 Bad Request` -- if box is locked
- **Note:** Guardians are managed through the invitation system, not directly through this endpoint.

#### `DELETE /boxes/owned/:boxId/guardian/:guardianId`
Remove a guardian from a box.

- **Auth:** Required (owner only)
- **Response:**
  - `200 OK` -- `{ "message": "Guardian deleted successfully", "guardian": { "id": ... } }`
  - `401 Unauthorized` -- if not the owner (returns `"You don't have permission to delete guardians from this box"`)
  - `400 Bad Request` -- if box is locked

---

### Box Service -- Locked Box Immutability

Once a box is locked (`isLocked: true`), the following operations all return **400 Bad Request** with an error message containing "immutable":

- Updating name, description, or unlock instructions
- Attempting to unlock (`isLocked: false`)
- Adding or updating documents
- Deleting documents
- Adding or updating guardians
- Deleting guardians

The **only** mutation allowed on a locked box is deletion by the owner.

---

### Box Service -- Shard Management (Lock Flow)

#### Lock Box with Shards
When locking a box, the owner distributes encrypted shards to each guardian:

- **Payload includes:** `shard_threshold` (min shards to unlock) and array of `shards` (one per guardian, each with `guardian_id`, `shard`, `shard_hash`).
- After locking: `isLocked = true`, `shardThreshold` and `totalShards` are set, each guardian's `encryptedShard` and `shardHash` are stored.

#### Fetch Guardian Shard
Guardians can fetch their encrypted shard:

- Returns `{ "encryptedShard": string }`.
- Does not delete the server-side copy (read-only operation).

#### Acknowledge Guardian Shard
After a guardian securely stores their shard locally, they acknowledge receipt:

- Server deletes the guardian's `encryptedShard` (sets to `None`).
- Sets `shardFetchedAt` timestamp on the guardian.
- Increments `shardsFetched` counter on the box.
- When all guardians acknowledge (shardsFetched == totalShards), `shardsDeletedAt` is set on the box.

---

### Box Service -- Guardian View Endpoints

#### `GET /boxes/guardian`
List all boxes where the authenticated user is a non-rejected guardian.

- **Auth:** Required
- **Response:** `200 OK` -- `{ "boxes": GuardianBox[] }`
- Each box includes: `guardiansCount`, `isLeadGuardian`, `pendingGuardianApproval`, `documents`, `guardians`.
- Returns empty array for non-guardian users.

#### `GET /boxes/guardian/:boxId`
Get a specific box as a guardian.

- **Auth:** Required (must be a guardian of the box)
- **Response:**
  - `200 OK` -- `{ "box": GuardianBox }`
  - `401 Unauthorized` -- if caller is not a guardian
  - `404 Not Found` -- if box does not exist

---

### Box Service -- Guardian Invitation Response

#### `PATCH /boxes/guardian/:boxId/invitation`
Accept or reject a guardian invitation.

- **Auth:** Required (must be the invited guardian)
- **Body:** `{ "accept": bool }`
- **Response:**
  - `200 OK` -- `{ "message": "Guardian invitation accepted/rejected successfully" }`
  - `400 Bad Request` -- `{ "error": "No pending invitation found for this user" }` if guardian status is not `Invited`
- On accept: guardian status changes to `Accepted`.
- On reject: guardian status changes to `Rejected`.

---

### Box Service -- Shard Acceptance

#### `POST /boxes/guardian/:boxId/shard/accept`
Guardian confirms they have securely stored their shard.

- **Auth:** Required (must be a guardian of the box)
- **Response:**
  - `200 OK` -- `{ "message": "Shard accepted successfully", "shardAcceptedAt": string, "boxId": string, "boxName": string }`
  - `200 OK` -- `{ "message": "Shard already accepted", "shardAcceptedAt": string }` (idempotent)
  - `401 Unauthorized` -- if not a guardian
  - `400 Bad Request` -- if box is not locked (error message contains "locked")
  - `404 Not Found` -- if box does not exist

---

### Box Service -- Unlock Request Flow

#### `PATCH /boxes/guardian/:boxId/request`
Initiate an unlock request (lead guardian only).

- **Auth:** Required (must be a **lead guardian** of the box)
- **Body:** `{ "message": string }`
- **Response:**
  - `200 OK` -- `{ "box": { ..., "unlockRequest": { "status": "requested", "message": string, "initiatedBy": string } } }`
  - `400 Bad Request` -- if caller is not a lead guardian
- Creates an `UnlockRequest` with status `requested`.

#### `PATCH /boxes/guardian/:boxId/respond`
Approve or reject an unlock request.

- **Auth:** Required (must be a guardian of the box)
- **Body:** `{ "approve": true }` or `{ "reject": true }`
- **Response:**
  - `200 OK` -- `{ "box": { ..., "unlockRequest": { "approvedBy": [...], "rejectedBy": [...] } } }`
  - `401 Unauthorized` -- if not a guardian
  - `4xx` -- if no active unlock request exists
  - `4xx` -- if payload is invalid (missing both `approve` and `reject`)
- Adds the guardian's user ID to `approvedBy` or `rejectedBy`.
- Non-guardians cannot modify the unlock request.

---

### Invitation Service

#### `POST /invitations/new`
Create a new invitation.

- **Auth:** Required
- **Body:** `{ "invitedName": string, "boxId": string }`
- **Response:** `200 OK` -- Full `Invitation` object
- Generates an 8-character uppercase alphabetic invite code (A-Z only).
- Sets expiry to 48 hours from creation.
- `opened: false`, `linkedUserId: null`.

#### `GET /invitations/me`
Get all invitations created by the authenticated user.

- **Auth:** Required
- **Response:** `200 OK` -- `Invitation[]`
- Returns only invitations where `creatorId` matches the caller.
- Returns empty array if none exist.

#### `GET /invitations/view/:inviteCode`
View an invitation by code (read-only, does not consume).

- **Auth:** Required
- **Response:**
  - `200 OK` -- Full `Invitation` object
  - `404 Not Found` -- if code does not exist
  - `410 Gone` -- if invitation has expired
- Does **not** mark the invitation as opened or link a user.
- Expiry timestamp is not modified by viewing.

#### `PUT /invitations/handle`
Redeem an invitation code.

- **Auth:** Required
- **Body:** `{ "inviteCode": string }`
- **Response:**
  - `200 OK` -- `{ "boxId": string, ... }`
  - `404 Not Found` -- if code does not exist
  - `410 Gone` -- if invitation has expired
  - `403 Forbidden` -- if invitation has already been redeemed
- Sets `opened: true` and `linkedUserId` to the caller's user ID.
- Publishes an SNS event (`invitation_viewed`) for downstream processing.
- **Concurrency safety:** If two users attempt to redeem the same code simultaneously, exactly one succeeds (200) and the other fails (403). The invitation is linked to only one user.

#### `PATCH /invitations/:id/refresh`
Refresh an invitation (generate new code and reset expiry).

- **Auth:** Required (must be the invitation creator)
- **Response:**
  - `200 OK` -- Updated `Invitation` object with new code and expiry
  - `403 Forbidden` -- if caller is not the invitation creator
- Generates a new 8-character invite code.
- Resets expiry to 48 hours from the refresh time (not from original creation).
- Resets `opened: false` and `linkedUserId: null`.

---

### Invitation Event Service (SNS Consumer)

Processes SNS events triggered by invitation actions. Runs as a Lambda consuming from an SNS topic.

#### Event: `invitation_created`
- **Trigger:** New invitation created.
- **Action:** Adds a new `Guardian` entry to the box with:
  - `id: ""` (empty until invitation is opened)
  - `name:` from `invited_name` field, or fallback `"Pending (CODE)"` if not provided
  - `status: Invited`
  - `leadGuardian:` from `is_lead_guardian` field
  - `invitationId:` from the event
- **Idempotency:** If a guardian with the same `invitationId` already exists, no duplicate is created.

#### Event: `invitation_viewed`
- **Trigger:** Invitation code redeemed by a user.
- **Action:** Updates the matching guardian entry:
  - Sets `id` to the `user_id` from the event
  - Changes `status` from `Invited` to `Viewed`
- **Graceful handling:** If no matching guardian is found, the handler logs a warning but does not error.

#### Malformed Events
- Invalid JSON in SNS messages is skipped without causing errors.
- Box data is not modified by malformed events.

---

### Invitation Code Properties

| Property | Value |
|---|---|
| Length | 8 characters |
| Alphabet | A-Z (uppercase only, 26 letters) |
| Total combinations | 26^8 = 208,827,064,576 |
| Uniqueness | Verified unique across batches of 1,000+ codes |
| Distribution | All 26 letters appear with reasonable frequency |
| Expiry | 48 hours from creation (or last refresh) |

---

## Data Store Layer

### BoxStore Trait

| Method | Description |
|---|---|
| `create_box(BoxRecord)` | Persist a new box |
| `get_box(id)` | Retrieve a box by ID |
| `get_boxes_by_owner(owner_id)` | Query boxes by owner (uses GSI) |
| `get_boxes_by_guardian_id(guardian_id)` | Query boxes where user is an accepted guardian |
| `update_box(BoxRecord)` | Update a box record |
| `delete_box(id)` | Delete a box |
| `scan_locked_boxes()` | Scan for all locked boxes (used by reminder service) |

- **Guardian filtering:** `get_boxes_by_guardian_id` returns only boxes where the guardian status is **not** `Rejected`.
- **Scan locked boxes:** Returns only boxes where `isLocked == true`.

### InvitationStore Trait

| Method | Description |
|---|---|
| `create_invitation(Invitation)` | Persist a new invitation |
| `get_invitation(id)` | Retrieve by primary key |
| `get_invitation_by_code(invite_code)` | Retrieve by invite code (uses GSI) |
| `update_invitation(Invitation)` | Update an invitation |
| `delete_invitation(id)` | Delete an invitation |
| `get_invitations_by_box_id(box_id)` | Query by box ID |
| `get_invitations_by_creator_id(creator_id)` | Query by creator (uses GSI) |

### PushTokenStore Trait

| Method | Description |
|---|---|
| `save_push_token(PushToken)` | Save or update a push token |
| `get_push_token(user_id)` | Get token by user ID |
| `get_push_tokens(user_ids)` | Batch get tokens |
| `delete_push_token(user_id)` | Delete a token |

---

## Microservices

| Service | Runtime | Trigger | Purpose |
|---|---|---|---|
| box-service | Lambda + API GW | HTTP | Core box, document, guardian CRUD and shard management |
| invitation-service | Lambda + API GW | HTTP | Invitation creation, redemption, refresh, viewing |
| invitation-event-service | Lambda | SNS | Processes invitation events to update box guardians |
| notification-service | Lambda | SNS | Sends push notifications via Expo |
| reminder-service | Lambda | CloudWatch Scheduled | Periodic reminders for locked boxes |

---

## Key Behavioral Rules

1. **Ownership authorization:** Box mutations (update, delete, add/remove documents, add/remove guardians) are restricted to the box owner. Accessing another user's box returns 401.
2. **Locked box immutability:** Once locked, a box cannot be modified except by deletion. All mutation attempts return 400 with "immutable" in the error.
3. **Lock is one-way:** There is no API to unlock a box by setting `isLocked: false`. The unlock flow is through the guardian request/approval mechanism.
4. **Lock idempotency:** Locking an already-locked box succeeds and preserves the original `lockedAt` timestamp.
5. **Guardian lifecycle:** Guardians are created via the invitation event flow, not directly. The status progresses: `Invited` -> `Viewed` -> `Accepted`/`Rejected`.
6. **Invitation single-use:** Each invite code can only be redeemed once. Concurrent redemption attempts are safe -- exactly one succeeds.
7. **Invitation expiry:** Codes expire after 48 hours. Expired codes return 410 Gone. Refreshing resets the 48-hour window.
8. **Shard cleanup:** After all guardians acknowledge their shards, the server copies are deleted and `shardsDeletedAt` is recorded.
9. **Event idempotency:** `invitation_created` events are idempotent -- duplicate events do not create duplicate guardians.
10. **Graceful error handling:** Malformed SNS events are skipped without causing Lambda errors.
