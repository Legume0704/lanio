# Lanio

A Stremio add-on that serves your local media files as streams over your Local Area Network (LAN). Point it at a directory of movies and TV shows, and they'll appear as stream options inside Stremio alongside any other sources you use.

Built with Rust to be hyper efficient with a low footprint on your server!

## How it works

On startup the server scans your media directory, parses filenames to extract title/year/episode info, and looks each one up via the TMDB API to get its IMDb ID. Stremio identifies content by IMDb ID, so once a file is matched it shows up as a stream option whenever you open that movie or episode.

After this initial scan, a file watcher watches the directory for any changes and makes adjustments in real time.

## Deployment and Installation

Please see [the wiki](https://github.com/Legume0704/lanio/wiki) for guides on deployment and installation, along with other helpful pages!

## License

MIT
