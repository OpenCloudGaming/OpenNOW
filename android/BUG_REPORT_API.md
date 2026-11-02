# Android bug-report API contract

The Android client sends a pseudonymous, installation-scoped `reporterId` with every bug report.
Use that value as the lookup key for report throttles or blocks. It is a namespaced SHA-256 digest;
the raw GFN device ID, account ID, and email address are not sent.

`reporterId` is useful for ordinary abuse control, but it is not a hardware-backed identity. Clearing
app data or reinstalling can create a new identifier, and a modified client can forge one. If stronger
enforcement is needed, combine it server-side with rate limits or authenticated account signals that
the service already has instead of asking the app to upload raw hardware identifiers.

## Custom rejection shown by the app

Return HTTP `403` with this JSON when the reporter is blocked:

```json
{
  "ok": false,
  "error": {
    "code": "REPORTER_BANNED",
    "message": "Bug reporting is disabled for this installation. Contact support if you believe this is a mistake.",
    "retryable": false
  }
}
```

The Android app displays `error.message` in its existing bug-report error panel. It normalizes
whitespace and limits the public message to 320 characters. It never displays an unstructured HTML
or proxy error body.

Use HTTP `429` and `code: "REPORT_RATE_LIMITED"` for a temporary throttle. Set `retryable` to `true`
and write the retry instruction in `message`.

## OpenAPI 3.1 schema

```yaml
openapi: 3.1.0
info:
  title: OpenNOW Android bug reports
  version: 1.0.0
paths:
  /releases/opennow/bug-reports:
    post:
      operationId: createOpenNowAndroidBugReport
      requestBody:
        required: true
        content:
          multipart/form-data:
            schema:
              type: object
              required:
                - title
                - description
                - versionName
                - versionCode
                - platform
                - reporterId
                - metadata
              properties:
                title:
                  type: string
                  minLength: 1
                description:
                  type: string
                  minLength: 50
                versionName:
                  type: string
                versionCode:
                  type: string
                platform:
                  type: string
                  const: android
                reporterId:
                  type: string
                  pattern: '^br1_[0-9a-f]{64}$'
                  description: Pseudonymous installation key used for abuse prevention.
                metadata:
                  type: string
                  contentMediaType: application/json
                  description: A JSON object serialized as a multipart text field.
                files:
                  type: array
                  maxItems: 5
                  items:
                    type: string
                    contentMediaType: application/octet-stream
                    maxLength: 10485760
      responses:
        '201':
          description: Report accepted.
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/BugReportAccepted'
        '400':
          description: Invalid report.
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/BugReportRejected'
        '403':
          description: This reporter is blocked.
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/BugReportRejected'
        '429':
          description: This reporter is temporarily rate-limited.
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/BugReportRejected'
components:
  schemas:
    BugReportAccepted:
      type: object
      required: [ok, reportId]
      properties:
        ok:
          type: boolean
          const: true
        reportId:
          type: string
    BugReportRejected:
      type: object
      required: [ok, error]
      properties:
        ok:
          type: boolean
          const: false
        error:
          type: object
          required: [code, message, retryable]
          properties:
            code:
              type: string
              maxLength: 80
            message:
              type: string
              minLength: 1
              maxLength: 320
            retryable:
              type: boolean
```

## Server-side block record

A minimal persistent record can use this shape:

```json
{
  "reporterId": "br1_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
  "enabled": true,
  "publicMessage": "Bug reporting is disabled for this installation.",
  "internalReason": "Repeated reports without actionable evidence",
  "createdAt": "2026-08-05T20:00:00Z",
  "expiresAt": null
}
```

Before accepting files, validate `reporterId`, load the enabled non-expired block, and return the
structured `403` response using `publicMessage`. Keep `internalReason` server-only. Store the
`reporterId` beside accepted reports so an administrator can copy it from a report into the block
table. Check the block before persisting attachments to avoid unnecessary storage and processing.

## Conversation reply attachments

`POST /releases/opennow/bug-reports/{reportId}/comments` accepts the existing JSON
reply format or `multipart/form-data`. Both formats require the reporter's installation
ID (the `X-Bug-Report-Reporter-Id` header or `reporterId` field) and `termsAccepted=true`.
Multipart requests use repeated `files` parts, up to 5 files of 10 MiB each. `comment`
is optional when files are present; the API supplies “Attached files.” for that message.
Text replies retain their 3,000-character limit.

The Android composer allows 4 selected files (20 MiB combined, 10 MiB each), with an
optional fifth attachment containing current sanitized debug logs captured at send time.
It retains the draft and selected files after an upload failure, and clears them after
success. File selection uses Android's document picker and supports logs, images,
videos, and other files.

Successful replies return HTTP 201 and the updated conversation under `data`.
Each comment includes `files` metadata (`id`, `name`, `contentType`, `size`, and
`downloadAvailable`). The report's aggregate `files` list also includes follow-up
attachments with `commentId`, so existing admin file downloads can find them. Android
shows those files under their message rather than under the original report.

TempAPI retains files only after the comment is stored, and cleans up rejected uploads.
Existing ownership, deletion, closed-conversation, reporter-ban, terms, and reply-rate
checks still apply. Parsed follow-up diagnostics are retained for admin AI reanalysis;
public replies return file metadata without those extra diagnostic summaries.

## Published bugs (admin-authored)

Published bugs are separate from private reporter conversations. In Release Manager, select an app
and open **Bug Posting** to publish or edit a formatted post. Bold text, red emphasis, HTTPS links,
combined bold red text, and line breaks use the same plain-text formatting contract as announcements. Users cannot post
text or comments here; they can upvote and share one current diagnostic export after consent.

| Status | App label | Progress |
| --- | --- | --- |
| `investigating` | Investigating | Admin-selected, 0–100 |
| `fixing` | Fix in progress | Admin-selected, 0–100 |
| `awaiting_review` | Fix ready · awaiting review | Admin-selected, 0–100 |
| `released` | Fixed · update to v2.3.3 | Always 100; `fixedVersion` required |

Public endpoints use `X-Bug-Report-Reporter-Id` with the existing installation-scoped ID:

- `GET /releases/:appSlug/published-bugs[?cursor=...]` returns `{ok,status,data:{bugs,nextCursor}}`.
  Pages contain up to 20 posts, newest first. Each post includes `id`, `title`, `body`, `status`,
  `progress`, `fixedVersion`, `acceptingLogs`, `votes`, `voted`, `logCount`, `createdAt`, and `updatedAt`.
- `PUT /releases/:appSlug/published-bugs/:bugId/vote` accepts JSON `{voted:true}` (or `false`).
  The explicit state makes retries idempotent; votes are unique per installation and bug.
- `POST /releases/:appSlug/published-bugs/:bugId/logs` accepts multipart fields `submissionId`
  (UUID), `termsAccepted=true`, and one `files` attachment (`.txt`, `.log`, or `.zip`, 1 byte–10 MB).
  Android supplies its sanitized diagnostics export; no free-text fields are submitted.
  Retry with the same submission ID after a transport failure to avoid duplicate diagnostics.
  Limits are five uploads per installation per hour, 20 attempts per IP per hour, and 5,000 logs
  per bug. Reporter bans apply to both contributions. Uploaded files are private.

Admin endpoints require the existing release-manager permission (`50`):

- `GET /releases/apps/:appSlug/published-bugs[?cursor=...]` lists up to 250 posts per page.
- `POST /releases/apps/:appSlug/published-bugs` publishes `{title,body,status,progress,fixedVersion,acceptingLogs}`.
- `PATCH /releases/apps/:appSlug/published-bugs/:bugId` edits the same fields.
- `GET /releases/apps/:appSlug/published-bugs/:bugId/logs.zip` streams all submitted diagnostics
  with a manifest; it does not buffer the archive in server memory or unpack uploaded ZIPs.
- `DELETE /releases/apps/:appSlug/published-bugs/:bugId` hides the post, removes every attached
  file, then deletes its logs, votes, and post. An I/O failure retains a hidden tombstone and
  asset records, shown as “Deletion incomplete” in Bug Posting. **Retry deletion** finishes cleanup;
  new votes and uploads cannot target a tombstone.

The schema is created idempotently by TempAPI's existing `ensureSchema` path. Assets live under
`database/uploads/published-bugs/`; preserve this directory with the releases database when backing
up or deploying. Install production dependencies (including `yazl`), deploy/restart TempAPI first,
then deploy PrintedSite and distribute the updated Android app. Publishing bugs works independently
of the private-report Discord webhook setting. Existing private reports are unchanged.

Admins can switch **Accept debug logs** off for an individual published bug without deleting its
existing diagnostics or votes. `acceptingLogs` defaults to true for existing posts. Paused bugs reject
new uploads with HTTP 409 and `error.code = LOG_COLLECTION_PAUSED`; retries of a previously stored
submission still receive their original success acknowledgement. The upload insert checks this flag
atomically, including when an admin pauses collection during an in-flight request. Android disables
Add logs, explains that collection is paused, and keeps upvoting available.

Awaiting-review bug details explain Google Play's review process and link to Google's Play Console
help page. Both announcement and published-bug editors include a **Bold red** formatting command;
`<red>**text**</red>` and `**<red>text</red>**` render as combined bold red emphasis in Android.
