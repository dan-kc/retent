# retent

`retent` is a Markdown-native incremental-learning queue. A vault contains `.md` files whose leading YAML front matter has `type: note|card` and an integer `priority: 1..=10`. Learning state is replayed from Markdown history tables; no database or cached scheduler fields are written.
