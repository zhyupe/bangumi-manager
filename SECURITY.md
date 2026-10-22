# Security and privacy

Security fixes target the latest Windows release. Other operating-system targets have not been validated for distribution.

## Reporting a vulnerability

If private vulnerability reporting is enabled on the [repository's Security page](https://github.com/zhyupe/bangumi-manager/security), use it to share reproduction steps and the affected version privately. Otherwise, open an issue requesting a private contact channel without including exploit details, credentials, or personal files. Do not attach an unredacted application database.

## Local data

The application can copy, move, link, rename, and remove media files according to confirmed tasks. Run it with the ordinary account that owns the library; administrator privileges are not required for normal use. Share permissions and some symbolic-link configurations may impose additional operating-system requirements.

Settings, title associations, task checkpoints, and caches are stored locally. The TMDB token is stored in the settings database without application-level encryption and is protected by the data directory's filesystem permissions. Protect database backups accordingly. The MusicBrainz contact address is included in requests to that service.

Recovery depends on retaining the task database, registered backups, and any staged rename files. Close the application before backing up or restoring its data directory. Keep old-version backups outside directories scanned by your media software.

## Sharing diagnostics

Timing logs omit tokens, search terms, request/response bodies, and local media paths. Exported task plans, error messages, screenshots, NFO files, and databases can still disclose file locations, titles, share names, or account information. Review and redact them before sharing; use fictional examples when possible.

Provider metadata and artwork come from external services. Their availability, accuracy, and terms are outside this application's control. The application does not provide a public network server or configure access to media servers or players.
