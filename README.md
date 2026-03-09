# Lanio

A Stremio add-on that serves your local media files as streams over your Local Area Network (LAN). Point it at a directory of movies and TV shows, and they'll appear as stream options inside Stremio alongside any other sources you use.

Built with Rust to be hyper efficient with a low footprint on your server.

## How it works

On startup the server scans your media directory, parses filenames to extract title/year/episode info, and looks each one up via the TMDB API to get its IMDb ID. Stremio identifies content by IMDb ID, so once a file is matched it shows up as a stream option whenever you open that movie or episode.

After this initial scan, a file watcher watches the directory for any changes and makes adjustments in real time.

## Setup

First, it must be noted that to install an Addon directly to Stremio, it MUST be `https` with a TLS certificate. However, Stremio does not care if the video streams themselves are plain `http`, which is how this addon works over the LAN. Throughout this guide, you will see the difference between Base and Public URLs. Base refers to the URL as reachable over the LAN (e.g. http://192.168.1.10:8078). Public URL is the `https` full domained URL of Lanio.

Here are three options (though not the only options) for deployment:

1. Deploy alongside a local instance of **AIOStreams** (Recommended)
2. Deploy with **Tailscale Funnel** (Handles TLS Certificates for you)
   - Example files contain commented out tailscale configuration for your convenience if you choose this route.
3. Route with a reverse proxy that serves a TLS Certificate for your domain
   - This is an advanced setup that will not be covered in this guide. I assume you know what you're doing if you go this route.

### Docker

It's highly recommended to run Lanio as a docker container. Example compose and env files can be found in [the examples folder](examples/). This assumes you already have docker installed.

1. Download the compose.yaml and .env.sample files (the ts-serve.json file is only needed for Tailscale configuration):

   ```bash
   curl -O https://raw.githubusercontent.com/Legume0704/lanio/refs/heads/main/examples/compose.yaml
   curl -o .env https://raw.githubusercontent.com/Legume0704/lanio/refs/heads/main/examples/.env.sample
   ```

2. Edit the files to your liking. `TMDB_API_KEY` is the only required environment variable to get running.

3. Run the following command:

   ```bash
   docker compose up -d
   ```

4. Access the addon at http://localhost:8078

### Environment Variables

| Variable       | Required | Default | Description                                                                                                                                      |
| -------------- | -------- | ------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| `TMDB_API_KEY` | Yes      | -       | TMDB API key ([get one free](https://www.themoviedb.org/settings/api))                                                                           |
| `PORT`         | No       | `8078`  | Port to listen on                                                                                                                                |
| `BASE_URL`     | No       | -       | Base URL used in video stream URLs (e.g. `http://192.168.1.10:8078`). Required if Stremio/AIOStreams can't reach `localhost`                     |
| `PUBLIC_URL`   | No       | -       | URL advertised for the manifest install link (home page and startup logs). Must be HTTPS if installing the addon directly in Stremio (see below) |

If neither `BASE_URL` nor `PUBLIC_URL` is set, stream URLs will use `http://localhost:{PORT}`.

## Install

### AIOStreams

[AIOStreams](https://github.com/Viren070/AIOStreams) is a self-hosted Stremio addon aggregator. It's the recommended method of installation because its authentication system is robust, adding another layer of security on top of Lanio. In addition, you only need to manage TLS certificate and domain configuration for AIOStreams, since AIOStreams can use a custom addon with plain HTTP, so Lanio can stay on plain HTTP on your local network.

[AIOStreams Deployment Guide](https://github.com/Viren070/AIOStreams/wiki/Deployment)

First, set up and start Lanio on your server with your media (See Setup section above).

Then, in your local AIOStreams instance, add this as a custom Addon:

1. Go to your AIOStreams configuration page
2. Go to **Addons**, **Marketplace**, and at the bottom click **Custom**'s Configure button
3. Configure with Lanio's Base URL
   ```
   http://<your-server-ip>:8078/manifest.json
   ```
4. Personally, I like to set Pin Position to Top and turn on Result Passthrough
5. Click Install
6. Save your configuration

As long as your local AIOStreams instance can reach Lanio with its Base URL, `PUBLIC_URL` is not needed in this setup.

### Tailscale Funnel

> ⚠️ WARNING ⚠️
>
> Lanio currently has no method of authentication. If you go this route, your media will be accessible on the public internet!

[Tailscale](https://tailscale.com/) is mainly a self-hosted VPN service. However, it has one killer feature that we will leverage here: [Funneling](https://tailscale.com/docs/features/tailscale-funnel). A Funnel reverse proxies to your tailscale node running on the server with a `ts.net` subdomain, handling all of the TLS certificate setup for you. This is exactly what we need for direct Stremio installation, and useful if you're on a CG-NAT with a nonunique public IP address (like me)

1. Create a Tailscale account if you don't have one already
2. Log in to your Tailscale admin panel. In the DNS section, choose a randomly-generated domain name.
3. Scrolling down the same page, enable HTTPS certificates.
4. In the [Access controls file](https://login.tailscale.com/admin/acls/file), add the following, after other rules. It should still be inside the top-level brackets (i.e. it should be surrounded by one level of brackets).
   ```
     "nodeAttrs": [
       {
         "target": ["autogroup:member"],
         "attr":   ["funnel"],
       },
     ],
   ```
5. In addition to the compose and env files, also download the `ts-serve.json` file.
   ```bash
   curl -O https://raw.githubusercontent.com/Legume0704/lanio/refs/heads/main/examples/ts-serve.json
   ```
6. Uncomment tailscale configuration in the compose file.
7. Edit all 3 files accordingly with your newly generated tailscale domain
   - Example: `PUBLIC_URL=https://lanio.your-ts-domain.ts.net`
   - Worth noting to also define a `BASE_URL` that is NOT your tailscale domain, to be used for video stream URLs. Funneling is bandwidth and rate limited (in my experience it's ~5mbps)
8. Get a [Tailscale auth key](https://login.tailscale.com/admin/settings/keys) and put it in your .env file
9. Compose up
   ```bash
   docker compose up -d
   ```
10. Navigate to your tailscale domain `https://lanio.your-ts-domain.ts.net`
    - Note it may take some time for configuration to propogate, typically around 10 minutes. Be patient with this.

### A combinitation of the two

This is what I personally do. Instead of Funneling directly to Lanio, I funnel to AIOStreams with a Lanio Addon configured there, along with all my other Addons.

Feel free to use the tailscale configuration in the compose file, change `TS_HOSTNAME` to what you prefer (what will show up in Tailscale and your Tailscale domain), and change `ts-config.json` to route to your AIOStreams instance instead.

## File structure

Filenames need to be parseable. Standard naming conventions work well:

**Movies**

```
The Dark Knight (2008).mkv
Oppenheimer.2023.1080p.BluRay.mkv
```

**TV shows** - put episodes in a folder named after the show:

```
Breaking Bad/
  Breaking.Bad.S01E01.Pilot.mkv
  Breaking.Bad.S01E02.mkv
Severance (2022)/
  Severance.S01E01.mkv
```

The folder name is used as the show title for TMDB lookup, so name folders clearly.

## Troubleshooting

**No streams appear** - Check `/health` to confirm files are indexed. Look at logs for TMDB lookup errors or filename parse failures.

**Wrong match** - TMDB lookup is best-effort. You can force the correct match by adding the IMDb ID anywhere in the filename (or folder name for TV shows):

```
Batman Begins tt0372784.mkv
The Dark Knight (2008) tt0468569.mkv
```

Find the IMDb ID for any title by opening its IMDb page, it's in the URL:
`https://www.imdb.com/title/`**`tt0372784`**`/`

When an IMDb ID is present in the filename, the TMDB title search is skipped entirely.

**Can't reach streams** - Set `BASE_URL` to the IP/hostname Stremio can actually reach. This is commonly needed when running on a NAS or server that's not `localhost` from Stremio's perspective.

**Didn't detect changes** - Restart Lanio and it will do a full scan of the media directory. I'm still working out the kinks so please report an issue and describe how exactly changes were made and I'll make adjustments to the file watcher.

## License

MIT
