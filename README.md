# MnemOS

This repository is for the MnemOS Operating System.

Currently, MnemOS is being rewritten as part of the v0.2 version. The current source may not
match the [currently published documentation](https://mnemos.jamesmunns.com)!

## Run the code

```
MELPOMENE_TRACE=trace cargo run
```

More details [in that comment](https://github.com/tosc-rs/mnemos/pull/15#issuecomment-1183739342).
## Missing dependencies

You may need to install extra dependencies for the embedded graphics sim. 

```console
sudo apt-get install libsdl2-dev
```
## Folder Layout

The project layout contains the following folders:

* [`assets/`] - images and files used for READMEs and other documentation
* [`book/`] - This is the source of the [currently published documentation], and is NOT up to date for v0.2.
* [`source/`] - This folder contains the source code of the kernel, userspace, simulator, and related libraries
* [`tools/`] - This folder contains desktop tools used for working with MnemOS

[`assets/`]: ./assets/
[`book/`]: ./book/
[`source/`]: ./source/
[`tools/`]: ./tools/

## License

[MIT] + [Apache 2.0].

[MIT]: ./LICENSE-MIT
[Apache 2.0]: ./LICENSE-APACHE
