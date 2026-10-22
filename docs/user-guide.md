# User guide

[English](user-guide.md) · [简体中文](user-guide.zh-CN.md)

## Titles and episode matching

Movies and TV shows default to TMDB when adding a title. Search by name or paste a TMDB movie/TV URL to specify the type. A numeric ID is tried in the selected type first; only a missing main entry triggers the other type. Authentication and connection failures are shown as errors.

Use the left side of the add window to search and select entries, and the right side to review details and confirm imports. Each side scrolls independently; narrow windows stack them vertically. After adding an entry, the window stays open with search results and related entries available, so you can continue adding other titles or seasons. Entries added in the current session are marked and cannot be submitted again; failed entries can be retried. Close the window when finished. Switching a TMDB episode group still closes the window after a successful confirmation.

Bangumi entries for different seasons can be linked to one local title. Confirm their season numbers yourself. Special episodes must be enabled and assigned explicitly; multiple sources describing the same episode do not create additional owned-episode counts.

Search results already in the library are marked. Selecting one automatically associates it with the existing local entry by provider, source type, and ID, including supported database IDs imported from NFO files. **Update local catalog information** refreshes that entry after confirmation, retaining its local title, media type, season mapping, and selected TMDB episode group. A different entry can still be linked manually as a new season.

To remove a title added by mistake, open its details in **Library** and choose **Delete title**. Downloaded episode metadata does not prevent deletion, but linked resources (including offline files), registered folders, unfinished tasks, file recovery records, or links from other titles do. After confirmation, the title and its pending matches are removed; source files, media, subtitles, backups, and finished task records are kept. You can add the title again from a data source.

For music, select an album and a provider. MusicBrainz searches return releases: use the artist, date, country, medium, and track list to choose the right edition. A release-group URL lists its releases rather than importing an arbitrary edition. Multi-disc numbering is retained. MusicBrainz requests respect the service's rate limit, so a cache miss may take longer than a cached result.

In **Organize**, select files before applying batch matching. Number matching supports both `sampleQe01.ts` and `Fictional.Series.S01E01.mkv`; review the selected number position and offset. The season summary below the selector helps compare enabled episodes with assignments. A file spanning several episodes needs its full end-episode range. Ambiguous names remain unassigned for manual review.

Number matching prioritizes `S01E09` markers and standalone bracketed numbers over title digits: `[Fictional Journey 2][09][720P].mp4` defaults to episode 9. Other extracted numbers remain available in the number-position selector. If the preferred episode is unavailable, it stays unmatched rather than falling back to a title number.

In a title's season view, drop a file onto an episode to preview the change on the current page. You can add other episodes, remove pending files, and review destinations, subtitles, backups, and conflicts before submitting. **Submit and view** creates the task and opens it; **Submit and run** queues it for execution while keeping the title page open. If files, associations, or naming settings change after preview, refresh the preview and review it again.

Use **Extras** in the title detail to drop files or directories, or choose them with the file and directory buttons. These imports use the same pending-file preview and submission actions, with the extra-file directory rules below, and do not count as owned episodes.

If a media file has a same-name NFO containing supported database IDs, use the NFO synchronization entry to preview and import that association. Existing NFO contents are metadata input, not instructions to execute commands.

Scanning recognizes files inside directories named exactly `bonus`, `extra`, `extras`, `cd`, `cds`, `scan`, or `scans` (case-insensitive) as extras. Selecting both files and a title assigns them as extras automatically; you can change the assignment manually. They do not consume episode or track numbers during batch matching. Rescan existing source directories to apply this detection.

Organizing extras preserves their subdirectories. Only an outermost recognized wrapper named `bonus`, `extra`, or `extras` is removed: `Bonus/Disc 1/cover.jpg` becomes `<extras root>/<title>/Disc 1/cover.jpg`, while `Scans/Book/01.jpg` becomes `<extras root>/<title>/Scans/Book/01.jpg`. Subtitles inside these folders appear as separate extras; select them as well. If final paths still collide, resolve the conflicts shown in the task preview before execution.

Standalone SRT, ASS, SSA, SUB, IDX, and VTT subtitles can use the same manual, numeric, sequence, or title matching in **Organize**. Import the corresponding movie or TV video first. Subtitles use its actual folder and filename, retaining a trailing two- or three-letter language marker and optional region/script suffix such as `.en`, `.chs`, `.cht`, `.eng`, or `.zh-Hans`. Multiple language markers joined with `&` are also supported, such as `.chs&jpn.ass`. Numeric endings such as `.01` remain episode numbers. Sequence matching counts each directory's language/format set separately. When only subtitles are selected, the season summary shows subtitle coverage.

The task preview shows the corresponding video and subtitle destination. Missing or changed videos, ambiguous versions, and partial matches to combined episodes prevent import. Existing subtitles require an explicit replacement decision and are backed up; the video and its NFO are preserved. Same-name subtitles found alongside an incoming video continue to follow that video's task automatically.

## Split movies and episodes

**Episode coverage** describes the episodes contained in one file, from the selected first episode through the selected last episode. It does not merge or split video. For a movie or episode split across multiple files, assign the files to the same movie or episode, then choose **Part 1**, **Part 2**, and so on under **Parts** (up to eight). Parts cannot also cover multiple episodes.

Explicit markers such as `Part1` and `Part2` are detected automatically and excluded from numeric episode matching. Sequential matching keeps same-name parts on the same episode. Movie filenames such as `Fictional Feature - 01.mkv` and `Fictional Feature - 02.mkv` are recognized only when same-directory siblings share the name and form a consecutive sequence starting at 01. Set the part manually when names are ambiguous or numbers are missing. For slash-separated chapter titles in one episode, name matching can compare the chapter corresponding to an explicit part number.

Imported filenames receive a ` - part1` suffix. Matching subtitles follow their part; standalone subtitles must select the same part, whose video must already exist. The season view accepts multiple explicitly numbered parts in one episode drop and previews them in place. Renaming preserves part numbers, and replacement affects only the selected part. Duplicate parts and mixing whole files with split parts prevent execution.

Episode counts are deduplicated and do not indicate whether all parts are present. Check the file list too. Multi-part naming and playback support depend on your media software and client. Check their requirements for movie and episode parts, container formats, audio tracks, and subtitles before organizing a collection.

## TMDB episode groups and renaming

Open a TV title and choose **Switch TMDB episode group**, then load the available groups. Compare the group name, type, episode counts, and numbering preview. You can return to TMDB's default seasons later. The selection is reused on subsequent synchronization.

Changing groups updates associations without immediately renaming media. Missing old episodes are retained but disabled. Active tasks, ambiguous associations, and combined files that no longer form a continuous range can prevent a switch. Resolve the reported conflict before proceeding.

After saving numbering changes, use **Rename library resources** in the title detail, the season's rename action, or the selected-resource action in **Resources**. Review old and new paths and NFO backups before creating and executing the task. The actual title folder is retained. Matching subtitles and per-file NFO files follow the media; shared NFO files stay in place and are updated after backup.

Unselected files at a destination are never implicitly overwritten. When names must be exchanged, the task temporarily stages the selected files before committing their destinations. Preserve these temporary files and the task's recovery data after interruption, and resume the task before changing associations or starting further operations.

## Inspecting and replacing resources

Scan registered libraries in **Resources → Media files**. Search by title, filename, or path, then combine extension, codec, size, resolution, and total-bitrate filters. Unknown values sort last and can be filtered separately. Offline folders keep their previous scan results.

Scanning does not run ffprobe. Probe selected files or all filtered results explicitly. At most two files are probed at a time; the batch can be cancelled and failed probes retried. Details distinguish total bitrate from video bitrate. An estimate based on size and duration is identified as such.

Use **Choose replacement file** on a resource or scan replacements for selected resources. The movie and season views offer the same workflow. Candidate scanning uses the files' actual directories, first matching basenames and then explicit title/episode ranges. All candidates are shown for your choice. Associate unlinked files with a title first.

Compare duration, resolution, codec, size, audio, subtitles, destination, backup location, and file operation. A larger file may be appropriate; more pixels alone do not establish better picture quality. Review duration changes above the greater of five seconds or 1%, resolution decreases, fewer tracks, or unavailable probe information before proceeding.

Only the selected old version is replaced. The destination keeps the old folder and basename, with the new file's extension. External files use the configured operation; an existing library candidate can be moved or accepted in place. The replacement itself is excluded from the old-version backup list.

Old external subtitles are retained when there are no new ones. Replacing a subtitle at the same path requires an explicit choice; check its timing against the new video. Shared NFO metadata is backed up and updated in place. Errors pause the queue and preserve backups and recovery checkpoints.

TS and M2TS describe containers, not video quality. Changing a container alone may not save much space. Perform any transcoding externally and compare quality and track compatibility before replacing the original.

## Backups and file operations

Under **Resources → Old-version backups**, select individual backups or a task. Cleanup lists exact paths and logical bytes before confirmation. Only registered, unchanged backups of fully completed tasks with verifiable replacement resources may be deleted. Failed or unverifiable tasks remain protected. Removing a completed task record does not remove its registered backups.

Cleanup is permanent and never automatic. Logical file sizes are not a measurement of freed disk space: hard links, compression, deduplication, and snapshots can affect physical storage use.

Choose copy, move, hard link, or symbolic link according to your storage setup. SMB operations use the supported server-side mechanism when applicable; failures are reported rather than silently discarding required checks. Confirm share access using the same Windows account as the application. A file-manager button next to a path opens its location, or the nearest existing parent for a destination not yet created.

## Media library compatibility and troubleshooting

Configure library scanning and local NFO reading in your media software. TV episode ordering should match the selected group. Supported NFO fields, naming conventions, audio, subtitles, and client capabilities vary. Test your actual client before adopting a new encoding across a library; this application neither transcodes nor controls media servers or players.

Open **Settings → Timing and logs**, reproduce the slow operation, then refresh the statistics. The panel shows active operations, durations, and failures. Logs are stored under the application's data folder in `logs/performance.jsonl`, with rotated history. Statistics restart with the application; log files remain.

For slow searches, compare provider/network timings with database waiting time. Remote queries can run during file organization; saving changed associations may still require the running task to stop. For a failed queue, inspect the task error, correct the cause, retry the failed task if needed, then explicitly continue the queue. See [security and privacy](../SECURITY.md) before sharing diagnostics.
