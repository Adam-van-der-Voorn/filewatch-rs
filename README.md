# filewatch-rs

A simple log watcher and aggregator

## Overview

filewatch-rs monitors specified files for changes and reads new content when files are updated. It's designed for real-time log monitoring and aggregation on a local machine.

## Features

- Watch multiple files simultaneously

## Usage

```bash
filewatch-rs <file1> <file2> ...
```

E.g. with cargo:

```bash
cargo run -- a.log b.log
```

## Build

```bash
cargo build --release
```