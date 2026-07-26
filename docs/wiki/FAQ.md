# FAQ

## Is this a cloud product?

Primary target is a **desktop** app. A remote API/web client may come later; it is not required for V1.

## Which databases are supported?

**V1 targets:** PostgreSQL, MongoDB, MySQL. Depth varies by milestone—see the PRD and kanban. Scaffold or experimental support for other engines may appear earlier.

## Are queries safe by default?

Yes. The query space defaults to **read-only**. Users may opt into **writes** where allowed. **DELETE** (and similar destructive remove) stays blocked until the product explicitly enables it. Validation is **per engine**, before the driver runs the statement.

## Where is the project board?

- Visual: `docs/kanban.html`  
- Text: `docs/KANBAN.md`  
- GitLab Issues / Boards if configured on the project  

## Where do I report a bug?

Open a **GitLab Issue** on the project. Include OS, steps to reproduce, and logs or screenshots when possible.

## Can I use this commercially?

See the [LICENSE](../../LICENSE) file (MIT unless changed by the project owners).
