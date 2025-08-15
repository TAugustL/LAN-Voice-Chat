# LAN-Voice-Chat
LAN voice chat in Rust.

## About
This program allows you to open a voice chat with someone on your local network.

## How to use
```
cargo run --release -- [MODE] [TARGET] (input device) (output device)
```

<table>
  <th>MODE</th>
  <th>TARGET</th>
  <tr>
    <td>
      <table>
        <th>-s | --server</th>
        <th>-c | --client</th>
          <tr>
            <td>start new server</td>
            <td>connect to server</td>
          </tr>
        </table>
    </td>
    <td>
      <table>
        <th>if SERVER</th>
        <th>if CLIENT</th>
          <tr>
            <td>Port to listen to (default: 8888)</td>
            <td>Address (IP:Port) to connect to</td>
          </tr>
      </table>
    </td>
  </tr>
</table>

You can also run ```cargo run --release``` to get this overview in your terminal.

> [!WARNING]
> This project is still in alpha state. You will encounter bugs, like echoes and endless feedback loops if you aren't carefull. I recommend both users use a headset for speaking, or alternatively but worse, I recommend the listener to mute themself while the speaker is talking. Also please ignore the static noise that you will hear when no one is talking :P (The reason is that I normalize all output audio to the same volume, so very quite noise will become much more noticable.)

<hr>

This project was made in the Rust 2024 Edition. Dependencies include smol (asynchronous processing), local-ip-address and cpal (audio processing). For more info see <a href="Cargo.toml">Cargo.toml</a>
