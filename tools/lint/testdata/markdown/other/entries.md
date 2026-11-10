# Entries

Intro prose that sits before every entry, so no count includes it.

#### `alpha.rs`

One two three four five.

Six seven eight nine ten eleven.

#### Beta entry

This prose is now in changelog voice, which only the default mode reports.

#### Gamma with a fence

Gamma prose that counts toward this entry and no other one.

```sh
echo these fenced words must not count at all
#### Not an entry heading
```

After the fence, the prose counts again for gamma, up to the next heading of level four or above.

#### Delta, with %s and %d in it

Delta has a few words.

### A section heading

Prose under a section heading belongs to no entry, however long it runs on.

#### Epsilon

Epsilon runs to the end of the file.
