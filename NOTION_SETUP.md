# Notion setup

## 1. Create an internal integration

1. Go to <https://www.notion.so/my-integrations> → **New integration**.
2. Type: *Internal*. Capabilities: **Read content** and **Update content**.
3. Copy the secret token (`ntn_…` / `secret_…`).

## 2. Share your task database with the integration

Open the task database in Notion → **⋯ → Connections → Connect to** → pick
your integration. Without this step the API returns `object_not_found`.

## 3. Find the database ID

From the database URL
`https://www.notion.so/myworkspace/8935f9d140a04f95a2cd9e26e2b25b56?v=…`
the ID is the 32-char hex segment (dashes optional).

## 4. Recommended database properties

| Purpose          | Suggested name   | Notion type            | Required |
|------------------|------------------|------------------------|----------|
| Task title       | Name             | Title                  | yes      |
| Status           | Status           | Status **or** Select   | yes      |
| Due date         | Due              | Date                   | no       |
| Priority         | Priority         | Select                 | no       |
| Pomodoro count   | Pomodoros        | Number                 | no*      |
| Tracked minutes  | Minutes          | Number                 | no*      |
| Last synced      | Last Synced      | Date                   | no*      |

\* Without these the app still works; those values just stay local-only and
are not pushed back.

## 5. Configure the app

Open **Settings → Notion**: paste the token (stored in the GNOME keyring) and
database ID, click **Validate & load properties** — the app fetches the
database, checks access, lists its properties, and lets you map each field.
Unmapped optional fields are simply skipped during sync.
