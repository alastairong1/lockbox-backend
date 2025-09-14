# lockbox-box-service

A simple service providing secure box storage with guardian management. This service allows users to manage boxes and guardians using a simple API managed with AWS Lambda and API Gateway. Boxes contain documents and associated unlock requests, and can be updated by authorized parties.

## Overview

The lockbox-box-service provides endpoints for managing boxes and associated unlock requests. Boxes can be created and updated by the owner, and guardians can assist with unlocking a box via approval or rejection. The service performs proper validations ensuring only authorized users may perform updates.

## API Endpoints

### Owner Endpoints

#### 1. Get Owned Boxes

**Endpoint:** `GET /boxes/owned`

**Headers:**
- `x-user-id`: Your user identifier

**Description:**
Returns all boxes owned by the user, including complete details such as documents, guardians, and unlock requests.

**Response Example:**
```json
{
  "boxes": [
    {
      "id": "box_id",
      "name": "Box Name",
      "description": "Description",
      "createdAt": "timestamp",
      "updatedAt": "timestamp",
      "isLocked": false,
      "unlockInstructions": "Instructions to unlock",
      "documents": [
        {
          "id": "doc_id",
          "title": "Document Title",
          "content": "Document content",
          "createdAt": "timestamp"
        }
      ],
      "guardians": [
        {
          "id": "guardian_id",
          "name": "Guardian Name",
          "leadGuardian": false,
          "status": "accepted",
          "addedAt": "timestamp"
        }
      ],
      "ownerId": "owner_user_id",
      "ownerName": "Owner Name",
      "unlockRequest": null
    }
  ]
}
```

#### 2. Create Box

**Endpoint:** `POST /boxes/owned`

**Headers:**
- `x-user-id`: Your user identifier

**Description:**
Create a new box with you as the owner.

**Payload Example:**
```json
{
  "name": "New Box",
  "description": "Description of the box",
  "ownerName": "Owner Name (optional)"
}
```

#### 3. Get Box

**Endpoint:** `GET /boxes/owned/{id}`

**Headers:**
- `x-user-id`: Your user identifier

**Description:**
Returns complete details of a specific box owned by the user, including all documents, guardians, and other metadata.

**Response Example:**
```json
{
  "box": {
    "id": "box_id",
    "name": "Box Name",
    "description": "Description",
    "createdAt": "timestamp",
    "updatedAt": "timestamp",
    "isLocked": false,
    "unlockInstructions": "Instructions to unlock",
    "documents": [
      {
        "id": "doc_id",
        "title": "Document Title",
        "content": "Document content",
        "createdAt": "timestamp"
      }
    ],
    "guardians": [
      {
        "id": "guardian_id",
        "name": "Guardian Name",
        "leadGuardian": false,
        "status": "accepted",
        "addedAt": "timestamp"
      }
    ],
    "ownerId": "owner_user_id",
    "ownerName": "Owner Name",
    "unlockRequest": null
  }
}
```

**Response Codes:**
- **200 OK:** Box retrieved successfully.
- **401 Unauthorized:** The user is not the owner of the box.
- **404 Not Found:** Box not found.

#### 4. Update Box (Owner Update)

**Endpoint:** `PATCH /boxes/owned/{id}`

**Headers:**
- `x-user-id`: Your owner user identifier

**Description:**
Allows box owners to update box details such as name and description.

**Payload Example:**
```json
{
  "name": "New Box Name",
  "description": "Updated description"
}
```

**Response Codes:**
- **200 OK:** Box updated successfully.
- **400 Bad Request:** Invalid request payload or missing required fields.
- **401 Unauthorized:** The user is not the owner or the box is not found.

#### 5. Delete Box

**Endpoint:** `DELETE /boxes/owned/{id}`

**Headers:**
- `x-user-id`: Your owner user identifier

**Description:**
Allows box owners to delete a box.

**Response Codes:**
- **200 OK:** Box deleted successfully.
- **401 Unauthorized:** The user is not the owner or the box is not found.
- **404 Not Found:** Box not found.

#### 6. Update Guardian

**Endpoint:** `PATCH /boxes/owned/{id}/guardian`

**Headers:**
- `x-user-id`: Your owner user identifier

**Description:**
Allows box owners to add or update a guardian for their box. This is the dedicated endpoint for managing individual guardians.

**Payload Example:**
```json
{
  "guardian": {
    "id": "guardian_id",
    "name": "Guardian Name",
    "leadGuardian": true,
    "status": "pending",
    "addedAt": "2023-05-25T12:00:00Z"
  }
}
```

**Response Example:**
```json
{
  "guardian": {
    "guardians": [
      {
        "id": "guardian_id",
        "name": "Guardian Name",
        "leadGuardian": false,
        "status": "accepted",
        "addedAt": "timestamp"
      },
      {
        "id": "guardian_id_2",
        "name": "Guardian Two",
        "leadGuardian": false,
        "status": "accepted",
        "addedAt": "2023-05-20T11:30:00Z"
      }
    ],
    "updatedAt": "2023-05-25T12:02:35Z"
  }
}
```

**Response Codes:**
- **200 OK:** Guardian updated successfully.
- **400 Bad Request:** Invalid request payload.
- **401 Unauthorized:** The user is not the owner of the box.
- **404 Not Found:** Box not found.

#### 7. Delete Guardian

**Endpoint:** `DELETE /boxes/owned/{id}/guardian/{guardian_id}`

**Headers:**
- `x-user-id`: Your owner user identifier

**Description:**
Allows box owners to delete a specific guardian from their box. This is a dedicated endpoint for removing individual guardians. If the guardian is also a lead guardian, they will be removed from both the guardians and lead guardians lists.

**Response Example:**
```json
{
  "message": "Guardian deleted successfully",
  "guardian": {
    "guardians": [
      {
        "id": "guardian_id_2",
        "name": "Guardian Two",
        "leadGuardian": false,
        "status": "accepted",
        "addedAt": "2023-05-20T11:30:00Z"
      }
    ],
    "updatedAt": "2023-05-25T12:02:35Z"
  }
}
```

**Response Codes:**
- **200 OK:** Guardian deleted successfully.
- **401 Unauthorized:** The user is not the owner of the box.
- **404 Not Found:** Box or guardian not found.

#### 8. Update Document

**Endpoint:** `PATCH /boxes/owned/{id}/document`

**Headers:**
- `x-user-id`: Your owner user identifier

**Description:**
Allows box owners to add or update a document for their box. This is the dedicated endpoint for managing individual documents.

**Payload Example:**
```json
{
  "document": {
    "id": "document_id",
    "title": "Document Title",
    "content": "This is the document content",
    "createdAt": "2023-05-25T12:00:00Z"
  }
}
```

**Response Example:**
```json
{
  "document": {
    "documents": [
      {
        "id": "document_id",
        "title": "Document Title",
        "content": "This is the document content",
        "createdAt": "2023-05-25T12:00:00Z"
      },
      {
        "id": "document_id_2",
        "title": "Another Document",
        "content": "Content of another document",
        "createdAt": "2023-05-20T11:30:00Z"
      }
    ],
    "updatedAt": "2023-05-25T12:02:35Z"
  }
}
```

**Response Codes:**
- **200 OK:** Document updated successfully.
- **400 Bad Request:** Invalid request payload.
- **401 Unauthorized:** The user is not the owner of the box.
- **404 Not Found:** Box not found.

#### 9. Delete Document

**Endpoint:** `DELETE /boxes/owned/{id}/document/{document_id}`

**Headers:**
- `x-user-id`: Your owner user identifier

**Description:**
Allows box owners to delete a specific document from their box. This is a dedicated endpoint for removing individual documents.

**Response Example:**
```json
{
  "message": "Document deleted successfully",
  "document": {
    "documents": [
      {
        "id": "document_id_2",
        "title": "Another Document",
        "content": "Content of another document",
        "createdAt": "2023-05-20T11:30:00Z"
      }
    ],
    "updatedAt": "2023-05-25T12:02:35Z"
  }
}
```

**Response Codes:**
- **200 OK:** Document deleted successfully.
- **401 Unauthorized:** The user is not the owner of the box.
- **404 Not Found:** Box or document not found.

### Guardian Endpoints

#### 1. Get Guardian Boxes

**Endpoint:** `GET /boxes/guardian`

**Headers:**
- `x-user-id`: Your user identifier

**Description:**
Returns all boxes where the authenticated user is a guardian (excluding rejected entries). Contains complete box details including documents, guardians, lead guardians, and guardian-specific information.

**Response Example:**
```json
{
  "boxes": [
    {
      "id": "box_id",
      "name": "Box Name",
      "description": "Description",
      "createdAt": "timestamp",
      "updatedAt": "timestamp",
      "isLocked": false,
      "ownerId": "owner_user_id",
      "ownerName": "Owner Name",
      "unlockInstructions": "Instructions to unlock",
      "documents": [
        {
          "id": "doc_id",
          "title": "Document Title",
          "content": "Document content",
          "createdAt": "timestamp"
        }
      ],
      "guardians": [
        {
          "id": "guardian_id",
          "name": "Guardian Name",
          "leadGuardian": false,
          "status": "accepted",
          "addedAt": "timestamp"
        }
      ],
      "unlockRequest": {
        "id": "unlock_request_id",
        "requestedAt": "timestamp",
        "status": "pending",
        "message": "Unlock request message",
        "initiatedBy": "guardian_id",
        "approvedBy": [],
        "rejectedBy": []
      },
      "pendingGuardianApproval": false,
      "guardiansCount": 3,
      "isLeadGuardian": true
    }
  ]
}
```

#### 2. Get Guardian Box

**Endpoint:** `GET /boxes/guardian/{id}`

**Headers:**
- `x-user-id`: Your guardian user identifier

**Description:**
Get a specific box where you are a guardian, including complete details of documents, guardians, and unlock information.

**Response Example:**
```json
{
  "box": {
    "id": "box_id",
    "name": "Box Name",
    "description": "Description",
    "createdAt": "timestamp",
    "updatedAt": "timestamp",
    "isLocked": false,
    "ownerId": "owner_user_id",
    "ownerName": "Owner Name",
    "unlockInstructions": "Instructions to unlock",
    "documents": [
      {
        "id": "doc_id",
        "title": "Document Title",
        "content": "Document content",
        "createdAt": "timestamp"
      }
    ],
    "guardians": [
      {
        "id": "guardian_id",
        "name": "Guardian Name",
        "leadGuardian": false,
        "status": "accepted",
        "addedAt": "timestamp"
      }
    ],
    "unlockRequest": {
      "id": "unlock_request_id",
      "requestedAt": "timestamp",
      "status": "pending",
      "message": "Unlock request message",
      "initiatedBy": "guardian_id",
      "approvedBy": [],
      "rejectedBy": []
    },
    "pendingGuardianApproval": false,
    "guardiansCount": 3,
    "isLeadGuardian": true
  }
}
```

**Response Codes:**
- **200 OK:** Box retrieved successfully.
- **401 Unauthorized:** The user is not a guardian for this box.
- **404 Not Found:** Box not found.

#### 3. Request Unlock (Lead Guardian Only)

**Endpoint:** `PATCH /boxes/guardian/{id}/request`

**Headers:**
- `x-user-id`: Your lead guardian user identifier

**Description:**
Allows lead guardians to initiate an unlock request for a box. The endpoint validates that the user is a lead guardian (and not rejected) of the box.

**Payload Example:**
```json
{
  "message": "Unlock request message"
}
```

**Response Codes:**
- **200 OK:** Unlock request initiated successfully, returning the updated guardian box details.
- **400 Bad Request:** Invalid payload or missing required fields.
- **401 Unauthorized:** The user is not an authorized lead guardian.
- **404 Not Found:** Box not found.
- **500 Internal Server Error:** An error occurred processing the update.

#### 4. Respond to Unlock Request (Guardian Only)

**Endpoint:** `PATCH /boxes/guardian/{id}/respond`

**Headers:**
- `x-user-id`: Your guardian user identifier

**Description:**
Allows guardians to respond to an existing unlock request. The endpoint validates that:
1. The user is a guardian (and not rejected) of the box
2. There is an active unlock request to respond to
3. The guardian hasn't already approved/rejected

**Payload Examples:**

_Approval:_
```json
{
  "approve": true
}
```

_Rejection:_
```json
{
  "reject": true
}
```

#### 5. Respond to Guardian Invitation

**Endpoint:** `PATCH /boxes/guardian/{id}/invitation`

**Headers:**
- `x-user-id`: Your user identifier

**Description:**
Allows users to accept or reject an invitation to be a guardian for a box. The endpoint validates that:
1. The user has a pending invitation for the box
2. The invitation hasn't already been responded to

**Payload Example:**
```json
{
  "accept": true
}
```

**Response Codes:**
- **200 OK:** Guardian invitation accepted successfully, returning the updated guardian box details.
- **400 Bad Request:** No pending invitation found for this box.
- **404 Not Found:** Box not found.
- **500 Internal Server Error:** An error occurred processing the response.

## Invitation Service

The lockbox-box-service includes an invitation service that allows users to create and manage invitations to boxes. This service facilitates the process of adding guardians to boxes through a user-friendly invitation flow.

### Invitation Endpoints

#### 1. Create Invitation

**Endpoint:** `POST /invitations/new`

**Headers:**
- `Authorization`: Bearer token with valid JWT

**Description:**
Creates a new invitation for a specific box. The creator becomes the owner of the invitation.

**Payload Example:**
```json
{
  "invited_name": "John Doe",
  "box_id": "box_id_to_invite_to"
}
```

**Response Example:**
```json
{
  "invite_code": "ABCDEFGH",
  "expires_at": "2023-06-01T14:30:00Z"
}
```

**Response Codes:**
- **200 OK:** Invitation created successfully.
- **400 Bad Request:** Invalid request payload.
- **401 Unauthorized:** User is not authenticated.

#### 2. Handle Invitation

**Endpoint:** `PUT /invitation/handle`

**Headers:**
- `Authorization`: Bearer token with valid JWT

**Description:**
Allows a user to accept an invitation and connect it to their account.

**Payload Example:**
```json
{
  "invite_code": "ABCDEFGH",
  "user_id": "accepting_user_id"
}
```

**Response Example:**
```json
{
  "message": "User successfully connected to invitation"
}
```

**Response Codes:**
- **200 OK:** Invitation handled successfully.
- **400 Bad Request:** Invalid request payload or expired invitation.
- **401 Unauthorized:** User is not authenticated.
- **404 Not Found:** Invitation not found.

#### 3. Refresh Invitation

**Endpoint:** `PATCH /invitations/:inviteId/refresh`

**Headers:**
- `Authorization`: Bearer token with valid JWT

**Description:**
Refreshes an existing invitation, generating a new code and extending the expiration time.

**Response Example:**
```json
{
  "invite_code": "NEWABCDE",
  "expires_at": "2023-06-03T14:30:00Z"
}
```

**Response Codes:**
- **200 OK:** Invitation refreshed successfully.
- **401 Unauthorized:** User is not authorized to refresh this invitation.
- **404 Not Found:** Invitation not found.

#### 4. Get My Invitations

**Endpoint:** `GET /invitations/me`

**Headers:**
- `Authorization`: Bearer token with valid JWT

**Description:**
Returns all invitations created by the authenticated user.

**Response Example:**
```json
[
  {
    "id": "invitation_id",
    "invite_code": "ABCDEFGH",
    "invited_name": "John Doe",
    "box_id": "box_id",
    "created_at": "2023-05-30T14:30:00Z",
    "expires_at": "2023-06-01T14:30:00Z",
    "opened": false,
    "linked_user_id": null,
    "creator_id": "creator_user_id"
  }
]
```

**Response Codes:**
- **200 OK:** Invitations retrieved successfully.
- **401 Unauthorized:** User is not authenticated.

## Project Architecture

The lockbox-box-service is built as a multi-service AWS Serverless application with the following components:

1. **Box Service**: Core service for managing boxes, documents, guardians, and unlock requests
2. **Invitation Service**: Service for handling the invitation process for adding guardians
3. **Shared Library**: Common code shared between services including models, authentication, and database access

### Technology Stack

- **Backend**: Rust with Axum web framework
- **Deployment**: AWS Lambda and API Gateway via SAM
- **Database**: Amazon DynamoDB
- **Authentication**: Amazon Cognito
- **CI/CD**: GitHub Actions

### Data Model

- **Boxes**: Contains documents and guardian relationships
- **Invitations**: Temporary invitations to become a guardian for a box, with user-friendly codes

## Running the Service

This service is designed exclusively for AWS Lambda and cannot be run as a standalone HTTP server.

## Deployment

This service is automatically deployed to AWS Lambda via GitHub Actions when changes are merged into the main branch. The deployment process includes:

1. Running tests to ensure code quality
2. Building the Rust code targeting Amazon Linux 2
3. Packaging the binary for Lambda deployment
4. Updating the Lambda function code

### Prerequisites for Deployment

To enable automatic deployment, you need to configure the following GitHub secrets:

- `AWS_ACCESS_KEY_ID`: AWS access key with permissions to update Lambda
- `AWS_SECRET_ACCESS_KEY`: AWS secret key
- `AWS_REGION`: AWS region where the Lambda function is deployed

### Manual Deployment

If you need to deploy manually, you can use AWS SAM:

```bash
# Install SAM CLI if you haven't already
brew install aws-sam-cli

# Build for Lambda
cargo build --release --target x86_64-unknown-linux-musl
mkdir -p target/lambda
cp target/x86_64-unknown-linux-musl/release/lockbox-box-service target/lambda/bootstrap
cd target/lambda && zip -j bootstrap.zip bootstrap

# Deploy using SAM
sam deploy --guided
```

## Testing

For testing the application, you can use:

```bash
cargo test
```

This will run all unit and integration tests defined in the codebase.

## CI/CD Pipeline

The service uses GitHub Actions for continuous integration and deployment:

- **CI Pipeline**: Runs on all pull requests and pushes to main
  - Code formatting check
  - Linting with Clippy
  - Unit tests

- **Deployment Pipeline**: Runs only on pushes to main branch after tests pass
  - Builds the application for Lambda
  - Packages the binary
  - Deploys to AWS Lambda

## Data Storage

The service uses DynamoDB to store box records. Key features include:
- Tables are defined in the CloudFormation template
- Global Secondary Index (GSI) for querying by owner_id
- Guardian relationships are stored in the box record

See the `GUARDIAN_INDEX_IMPLEMENTATION.md` file for details on future improvements to guardian search functionality.

## Additional Notes

- All timestamps are in ISO8601 format.
- Ensure the `x-user-id` header is included in requests for proper authentication and authorization.
- Box records include both owner information and guardian relationships.

## Dependencies

- Rust
- Cargo
- AWS Lambda runtime
- DynamoDB
- AWS SAM for local testing and deployment
