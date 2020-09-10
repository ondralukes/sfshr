# sfshr
**sfshr** (**s**ecure **f**ile **sh**a**r**e) is a command-line tool to share end-to-end encrypted (256-bit AES CBC) files using a link
## Usage
### Uploading
```
# Upload file test
sfshr test

# Upload file test without encryption
sfshr -n test
sfshr --no-encryption test
```
### Downloading
Paste generated link to command-line
```
# Encrypted link
sfshr -r b6s7cmB1vr5Hd3EjJn5bO88N8cpLoYgQng5yYNwWhTf0BUPGDeaMGMY5BEmoYe9KrcAEjdmCbl0lhxN8uIxwpg==

# Unencrypted link
sfshr --no-encryption -r 6yqAvYuVLlBgBkrYWQCyrBjFE1DAt9Tgk8Mir0zLrIs=
```

You can also upload whole directories.
### Options
* `-t --tar [tarname]` - store downloaded tar as `[tarname]`, instead of unpacking it
* `-n --no-encryption` - do not encrypt or decrypt the file
* `-q --quiet` - do not print anything (except download key)
* `-s --server [hostname:port]` - specify sfshr server (default: `ondralukes.cz:40788`