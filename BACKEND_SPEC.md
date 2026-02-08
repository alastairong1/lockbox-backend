# Lockbox Backend Specification

## 1. Purpose

Lockbox is a secure document vault. Users create encrypted containers called **boxes**, appoint trusted **guardians** to safeguard them, and lock them with a secret-sharing scheme. Boxes can only be unlocked through a multi-guardian approval process, ensuring documents remain protected until a legitimate emergency arises.

---

## 2. User Roles

| Role | Description |
|---|---|
| **Owner** | Creates and manages boxes. Full control over documents, guardians, and locking. |
| **Guardian** | A trusted person assigned to a box. Can approve or reject unlock requests. |
| **Lead Guardian** | A guardian with elevated privileges who can initiate unlock requests on behalf of the group. |

A single user may be an owner of some boxes and a guardian of others.

---

## 3. Core Concepts

### 3.1 Box

A box is a secure container that holds encrypted documents and is protected by a set of guardians.

- Has a **name**, **description**, and optional **unlock instructions** for guardians.
- Tracks its **locked/unlocked** state and the timestamp of when it was locked.
- Contains zero or more **documents** and zero or more **guardians**.
- Uses **optimistic concurrency** to prevent conflicting writes.

### 3.2 Document

A document is an encrypted payload stored within a box.

- Has an **id**, **title**, and **encrypted content**.
- Documents are opaque to the backend -- content is encrypted client-side before storage.

### 3.3 Guardian

A guardian is a trusted individual assigned to protect a box.

- Progresses through a lifecycle: **Invited** -> **Viewed** -> **Accepted** or **Rejected**.
- May be designated as a **lead guardian**, granting the ability to initiate unlock requests.
- Guardians are always added through the **invitation flow**, never directly.
- After locking, each guardian holds an **encrypted shard** that is part of the secret needed to unlock the box.

### 3.4 Invitation

An invitation is a time-limited code that allows a person to become a guardian of a box.

- Contains an **8-character alphabetic code** (A-Z uppercase).
- **Expires 48 hours** after creation or last refresh.
- Can be **viewed** (read-only preview) or **redeemed** (consumes the invitation).
- Each code can only be redeemed by **exactly one user**.
- Can be **refreshed** by the creator to generate a new code and reset the expiry window.
- Specifies whether the invitee will be a regular guardian or a **lead guardian**.

### 3.5 Unlock Request

An unlock request is the formal process by which guardians collectively authorize access to a locked box.

- Can only be **initiated by a lead guardian**.
- Includes an optional **message** explaining the reason.
- Other guardians **approve** or **reject** the request.
- Status progresses: **Requested** -> **Approved** / **Rejected** / **Completed**.

### 3.6 Shard (Secret Sharing)

When a box is locked, the owner splits a secret into encrypted shards distributed to guardians.

- A **threshold** defines the minimum number of shards required to reconstruct the secret.
- Each guardian receives one **encrypted shard** and a **hash** for verification.
- Guardians **fetch** their shard and then **acknowledge** receipt, at which point the server-side copy is deleted.
- Once all guardians have acknowledged, the server confirms that no shard data remains.

---

## 4. Business Rules

### 4.1 Authentication & Authorization

- All operations require an authenticated user.
- Unauthenticated requests are rejected.

### 4.2 Ownership

- Only the box **owner** can: create, update, delete boxes; add, update, or delete documents; remove guardians.
- Attempting to access or modify another user's box is rejected as unauthorized.

### 4.3 Locked Box Immutability

Once a box is locked:

- **No modifications** are allowed to the box name, description, unlock instructions, documents, or guardian list.
- The box **cannot be unlocked** by simply setting it to unlocked -- unlocking must go through the guardian approval process.
- **Locking is idempotent** -- locking an already-locked box succeeds without changing the original lock timestamp.
- The **only permitted mutation** on a locked box is deletion by the owner.

### 4.4 Guardian Lifecycle

1. Owner creates an **invitation** for a person.
2. The system automatically adds a placeholder **guardian** to the box (status: `Invited`).
3. The invitee **redeems** the invitation code, linking their user identity (status: `Viewed`).
4. The guardian **accepts** or **rejects** the invitation (status: `Accepted` or `Rejected`).
5. Rejected guardians are excluded from box queries and cannot participate in unlock flows.

Guardians cannot be added to a box by any means other than the invitation flow.

### 4.5 Invitation Rules

- Invite codes are **8 characters**, uppercase **A-Z only** (~209 billion combinations).
- Codes **expire after 48 hours**. Expired codes cannot be viewed or redeemed.
- Each code is **single-use** -- once redeemed, it cannot be redeemed again.
- **Concurrent redemption** is safe: if two users attempt to redeem the same code simultaneously, exactly one succeeds and the other is rejected.
- The creator can **refresh** an invitation to generate a new code and reset the timer, clearing any prior redemption.

### 4.6 Unlock Request Flow

1. A **lead guardian** initiates an unlock request with an optional message.
2. **Regular guardians** (non-lead) cannot initiate requests.
3. Each guardian **approves** or **rejects** the request. Their response is recorded.
4. **Non-guardians** cannot respond to an unlock request.
5. Responding when **no active unlock request** exists is rejected.
6. Invalid response payloads (missing both approve and reject) are rejected.

### 4.7 Shard Distribution & Cleanup

1. When locking, the owner provides a shard for each guardian along with a **threshold** (minimum shards to unlock).
2. Each guardian can **fetch** their shard (read-only, does not delete the server copy).
3. Each guardian **acknowledges** receipt, which deletes the server-side copy and records the acknowledgment timestamp.
4. A running count tracks how many guardians have acknowledged.
5. When **all guardians** have acknowledged, the system records that all server-side shard copies have been purged.

### 4.8 Shard Acceptance

- Guardians can **confirm** they have securely stored their shard locally.
- Acceptance is **idempotent** -- confirming again returns the original timestamp.
- Shards can only be accepted on **locked** boxes.

### 4.9 Event Processing

- When an invitation is **created**, a guardian placeholder is automatically added to the box.
  - This operation is **idempotent** -- duplicate events do not create duplicate guardians.
- When an invitation is **redeemed**, the guardian's identity is linked and their status is updated.
  - If the guardian entry is not found, the event is handled gracefully without error.
- Malformed events are skipped without disrupting the system.

### 4.10 Push Notifications

- Users can register a **push token** (device token for mobile notifications).
- Tokens can be saved, retrieved, batch-retrieved, or deleted per user.
- The system sends notifications on relevant events (e.g., unlock requests, invitation actions).

### 4.11 Reminders

- The system periodically scans for **locked boxes** and sends reminders as needed.

---

## 5. API Contract Summary

All endpoints require authentication. Responses use JSON with `camelCase` field names.

### 5.1 Owner -- Box Management

| Operation | Method | Path | Success | Key Errors |
|---|---|---|---|---|
| List my boxes | GET | `/boxes/owned` | 200 | 401 |
| Get a box | GET | `/boxes/owned/:boxId` | 200 | 401, 404 |
| Create a box | POST | `/boxes/owned` | 201 | 422 (invalid payload) |
| Update a box | PATCH | `/boxes/owned/:boxId` | 200 | 401, 400 (locked) |
| Delete a box | DELETE | `/boxes/owned/:boxId` | 200 | 401 |

**Create** requires `name` and `description`. Box is created unlocked with no documents or guardians.

**Update** supports partial payloads -- only provided fields are changed. Supports `name`, `description`, `isLocked`, and `unlockInstructions`. Setting `unlockInstructions` to `null` clears it.

### 5.2 Owner -- Document Management

| Operation | Method | Path | Success | Key Errors |
|---|---|---|---|---|
| Add/update document | PATCH | `/boxes/owned/:boxId/document` | 200 | 401, 422, 400 (locked) |
| Delete document | DELETE | `/boxes/owned/:boxId/document/:docId` | 200 | 401, 404, 400 (locked) |

Documents are upserted by `id` -- if a document with the same ID exists, it is replaced.

### 5.3 Owner -- Guardian Management

| Operation | Method | Path | Success | Key Errors |
|---|---|---|---|---|
| Remove guardian | DELETE | `/boxes/owned/:boxId/guardian/:guardianId` | 200 | 401, 400 (locked) |

Guardians can only be **added** via the invitation flow. Direct guardian creation is rejected (422).

### 5.4 Guardian -- Box Access

| Operation | Method | Path | Success | Key Errors |
|---|---|---|---|---|
| List my guardian boxes | GET | `/boxes/guardian` | 200 | 401 |
| Get a guardian box | GET | `/boxes/guardian/:boxId` | 200 | 401, 404 |

Guardian box responses include: total guardian count, whether the caller is a lead guardian, and whether the caller has a pending invitation.

### 5.5 Guardian -- Invitation Response

| Operation | Method | Path | Success | Key Errors |
|---|---|---|---|---|
| Accept/reject invitation | PATCH | `/boxes/guardian/:boxId/invitation` | 200 | 400 (no pending invite) |

Body: `{ "accept": true }` or `{ "accept": false }`.

### 5.6 Guardian -- Shard Acceptance

| Operation | Method | Path | Success | Key Errors |
|---|---|---|---|---|
| Accept shard | POST | `/boxes/guardian/:boxId/shard/accept` | 200 | 401, 400 (not locked), 404 |

Idempotent -- re-accepting returns the original acceptance timestamp.

### 5.7 Guardian -- Unlock Requests

| Operation | Method | Path | Success | Key Errors |
|---|---|---|---|---|
| Initiate unlock request | PATCH | `/boxes/guardian/:boxId/request` | 200 | 400 (not lead guardian) |
| Respond to request | PATCH | `/boxes/guardian/:boxId/respond` | 200 | 401, 400 (no request / invalid payload) |

Only **lead guardians** can initiate. Any guardian (including leads) can respond.

### 5.8 Invitations

| Operation | Method | Path | Success | Key Errors |
|---|---|---|---|---|
| Create invitation | POST | `/invitations/new` | 200 | 401 |
| List my invitations | GET | `/invitations/me` | 200 | 401 |
| View by code | GET | `/invitations/view/:code` | 200 | 404, 410 (expired) |
| Redeem code | PUT | `/invitations/handle` | 200 | 404, 410 (expired), 403 (already used) |
| Refresh invitation | PATCH | `/invitations/:id/refresh` | 200 | 403 (not creator) |

---

## 6. Key Invariants

1. A locked box is **immutable** -- only deletion is allowed.
2. Locking is **one-way** and **idempotent**.
3. Guardians are **only created through invitations**, never directly.
4. Invitation codes are **single-use** with **concurrent-safe** redemption.
5. Guardian invitation processing is **idempotent** -- duplicate events have no effect.
6. Shard acceptance is **idempotent** and only valid on locked boxes.
7. Only **lead guardians** can initiate unlock requests.
8. Only **guardians** can respond to unlock requests.
9. The server **purges shard data** after all guardians acknowledge receipt.
10. All resources are **scoped to the authenticated user** -- no cross-user access is possible.
