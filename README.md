Guillotine
==========

This project is based on ruma-gtk https://github.com/jplatte/ruma-gtk

But derives in a new one using directly the matrix.org API.

![screenshot](https://github.com/danigm/guillotine/blob/master/screenshots/guillotine.png)

## Supported m.room.message (msgtypes)

msgtypes          | Recv                | Send
--------          | -----               | ------
m.text            | Done                | Done
m.emote           |                     |
m.notice          |                     |
m.image           | Done (only preview) |
m.file            |                     |
m.location        |                     |
m.video           |                     |
m.audio           |                     |

Full reference in: https://matrix.org/docs/spec/client\_server/r0.2.0.html#m-room-message-msgtypes
