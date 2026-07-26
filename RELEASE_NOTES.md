# Release Notes

## Fixes

- The cookie status page failed to load, showing `invalid type: integer ... expected a string` and a total of 0 cookies. Reset boundaries are held as timestamps internally, and were sent that way whenever the live usage lookup did not answer, where the interface expects the written-out form. The page reads the whole listing as one piece, so a single cookie the lookup could not reach hid every other cookie with it. Boundaries that are already known are now shown even when the lookup fails, instead of the field going blank.

- A non-ASCII admin password left the web interface blank. The saved token is shortened for display by keeping a few characters at each end, but the length it was checked against was measured in bytes, so a password of four Chinese characters looked long enough to shorten and the cut landed inside a character. The page went down as it loaded, and because the token is read back from browser storage, reloading did not help. Shortening now counts characters throughout.

- The configuration file could be truncated when two saves overlapped. It was rewritten in place, so anything reading it — including ClewdR's own startup — could catch it after it had been emptied and before it was filled again. Saves now write a temporary file and rename it into place, which cannot be observed half-finished, and the file is created with owner-only permissions rather than having them applied after the contents are already on disk.

- A cookie whose cooldown had expired was returned to rotation without being recorded, so restarting in that window listed it as exhausted until the next request moved it back.

## Docker

- The image builds against wreq 6.0.0-rc.29 again. 0.13.1 had to hold it at rc.28, because rc.29 changed its TLS backend to one that carries BoringSSL as C++ and the image had no C++ compiler for musl. What was wanted then was a musl toolchain that CI could reach; it turned out to already exist as a published image, so the build now starts from one that carries a complete musl GCC.

- Image builds reuse cached layers between runs. Caching had been switched off, so every build recompiled all dependencies, BoringSSL included.

- Pull requests build the image for both architectures but no longer publish it. Builds from forks failed at the registry login, which cannot succeed for a fork, and every other pull request was pushing an image nobody asked for.

## Development

- Replaced the cookie actor with a mutex-guarded pool, removing the `ractor` dependency. None of the actor's handlers ever awaited, so the framework was supplying serialized access to shared state and nothing more.

- The pool now owns the cookies outright. They used to be copied back into the global configuration on every change, which is why saving settings had to carry them across by hand to avoid wiping them.

- Dropped `struct_iterable`, `rustls` and `aws-lc-rs`, none of which were used. The latter two were left behind by the removal of Gemini support; HTTP goes through wreq, which brings its own TLS.

- Pull requests no longer cross-compile the full release matrix. Nine binaries were built, uploaded and discarded a day later on every pull request. A single host build now covers the release profile and the embedded frontend, and the matrix runs on tags, where the artifacts are actually used.
