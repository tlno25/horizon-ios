# HorizonLauncherKit

One SwiftUI overlay, bundled inside the native Rust game app. `Present.swift`
exposes `horizon_launcher_present` to `horizon-ios-launcher`; `Bridge.swift`
passes asynchronous JSON requests to the game's Winit event loop. The same
Documents sandbox stores downloads, extracted resources, saves and options.

The publisher sets `HorizonResourcesManifestURL` in `GameInfo.plist` to an HTTPS
manifest whose `minecraft`, `url`, `sha256` and `size` fields describe the
**privately authorized** prepared Horizon game ZIP. Users only press Download.
No unverified third-party mirror or Minecraft copyrighted assets are bundled.

## Microsoft sign-in and Java Edition ownership (experimental)

The iOS launcher now supports the standard Microsoft OAuth device-code flow:
Microsoft -> Xbox Live -> XSTS -> Minecraft Services -> Java Edition entitlement
and profile. The sign-in page opens in the external browser, and only the
Microsoft public **application ID** is embedded in the app (never a client
secret). The refresh token and short-lived Minecraft token are kept in the
**iOS Keychain**, with `ThisDeviceOnly/WhenUnlocked` access. The app rechecks
ownership before Play or the built-in Mojang game download. The optional
legacy diagnostic download also checks ownership. Play passes a short-lived
Minecraft session over the local in-process Swift-to-Rust bridge,
not via logs, command-line arguments, or Documents.

**The distributor must register and obtain authorization for its own Microsoft
public client app and Minecraft services access.** Configure GitHub Actions
repository variable `HORIZON_MSA_CLIENT_ID` with its GUID. `game` and
`game-simulator` manual builds populate `HorizonMicrosoftClientID` in the
generated Info.plist. The ID is public metadata; do not add client secrets or
tokens to the repository, workflow variables, URLs, or assets. Until an ID and
required provider approval exist, the login button is unavailable. Neither
Microsoft sign-in nor ownership checks have been exercised with a real account
on iOS at this stage.

`launcher-start` will not start the native Horizon renderer without both a
validated, prepared resource tree and a verified Minecraft session. The game
keeps using the **existing `horizon-client`** renderer/loader and Winit event
loop; this work does not introduce a second app or a separate game simulation.

## Obtaining original files directly from Mojang

After sign-in, the launcher can obtain the **original** version JSON, client and
server JARs, Java libraries and assets from the official Mojang HTTPS servers,
under `Documents/official-minecraft`. SHA-1 checksums and sizes from Mojang's
version and asset-index manifests are checked; verified files are reused on
subsequent runs. These files are not shared with a server or published in the
IPA. Mojang's SHA-1 is used only to verify official distribution metadata, not
to secure passwords or account tokens.

**This does not turn original files into playable Horizon resources.** As of
this implementation, `horizon-prepare` still runs version-pinned Java exporters
and data generators that require **JDK 25 and a PC**. The iOS game cannot
produce `facts.json`, block registries, GPU mesh tables etc. directly from the
original JARs. The original-files button is an archive/export tool, not an
alternate Play or Install button. In particular, a successful download must
not change `launcher-state.ready` or silently enter the game.

To complete the existing, authorized native-game launch path:

1. Use a Windows/macOS/Linux PC with Rust, JDK 25, the pinned game files from
   your legitimately owned Minecraft Java Edition, and this Horizon source.
   Original files saved on the phone can be copied to the PC via the Files app
   (their directory is laid out like the official launcher).
2. Run the pinned preparation step, for example from the repository root:

   ```sh
   cargo run --release -p horizon-prepare -- --prepare --minecraft-dir /path/to/official-minecraft --server-jar /path/to/official-minecraft/server.jar
   cargo run --release -p horizon-prepare -- --check
   ```

3. Create a ZIP **from the contents** of `.local/horizon-client/<version>/` so
   `horizon-resources.json` is at its archive root. Keep the ZIP **private**.
   Install it using the launcher's personal HTTPS ZIP link plus SHA-256, or
   another private, authorized distribution channel with integrity checks.
4. On iPhone, with the approved Microsoft client ID configured, sign in with
   the Minecraft-owning account, import that prepared ZIP, and tap **Play**.
   This invokes the existing Rust native-game loading path, which still needs
   actual physical-device validation for rendering and full gameplay.

`HORIZON_IOS_RESOURCES_MANIFEST_URL` can point to a publisher-managed HTTPS JSON
manifest for one-tap delivery of a **privately authorized** prepared ZIP.
Never publicly mirror or embed proprietary game archives or credentials. A
license check at Play does not, by itself, authorize publishing Mojang's files.

Both `game` and `game-simulator` builds use macOS Actions and are manual-only.
An unsigned IPA still requires legitimate Apple code signing before it can run
on an iPhone. CI without private game files tests the app and its security gates,
not full Java-authenticated play.
