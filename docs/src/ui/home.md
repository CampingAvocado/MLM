# Home

The Home page is the control panel for MLM's background jobs. MLM runs these on its own on a schedule. This page shows when each one last ran and lets you start one right away.

At the top you are greeted by name. If something is wrong that stops MLM from working, a red message appears here:

- **mam_id is invalid** – MLM cannot log in to MyAnonamouse, so all features are switched off until you fix the `mam_id` in your configuration.
- **no qbittorrent instances configured** – MLM has no torrent client to talk to, so all features are switched off.

## The boxes

Each job has its own box showing **Last run** (the time, or "never") and **Result** (*success*, an error message, or *running* while it is busy). Most boxes have a **run now** button.

- **Autograbber** – one box for each autograbber in your configuration. It searches MyAnonamouse for new torrents that match your wishes and selects them for download.
- **Snatchlist Grabber** – one box for each snatchlist entry in your configuration. It picks up torrents you have already snatched on MyAnonamouse.
- **Goodreads Import** – one box for each Goodreads list in your configuration. It reads the list and looks for the books on it.
- **Linker** – places finished downloads in your library folders with tidy names and structure.
- **Cleaner** – removes older versions from your library when a better one arrives. It has no button, because it runs by itself after the linker.
- **Torrent downloader** – sends the selected torrents to your torrent client.
- **Audiobookshelf Matcher** – matches your torrents with the books in Audiobookshelf.

The page updates by itself while it is open, so you can watch a job go from *running* to *success*.
