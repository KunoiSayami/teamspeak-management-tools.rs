# TeamSpeak management tools

![License](https://img.shields.io/github/license/KunoiSayami/teamspeak-management-tools.rs?style=for-the-badge) ![GitHub release (latest SemVer)](https://img.shields.io/github/v/release/KunoiSayami/teamspeak-management-tools.rs?style=for-the-badge)

This is a simple Rust implement of TeamSpeak 3 Auto-Channel and User monitor.

## Features

You and other users can get a temporary channel automatically when you join the specified channel.

You can receive a message when user enter or left your server on [telegram](https://telegram.org/).


## Configuration

You should create a configure files in the same directory as work directory.


```toml
additional = [] # Additional configure filename
[server]
server-id = 1 # Server ID
channel-id = [1, 2] # Channel ID
privilege-group-id = 5 # Channel Privilege Group ID
redis-server = "" # Redis Server Address
leveldb = "" # LevelDB database file name/path
# track-channel-member = ""

# [mute-porter]
# enable = false
# monitor = 1
# target = 1
# Should use database ID
# whitelist = []

# [[permissions]]
# channel-id = 1
# it means set i_channel_needed_modify_power to 75 and i_channel_needed_delete_power to 60
# See: https://github.com/KunoiSayami/teamspeak-autochannel.rs/wiki/Permission-List for more key information
# map = [[125, 75], [133, 60]]

[telegram]
api-key = ""
target = 0
# api-server = ""
# responsible = false
# allowed-chat = []

[misc]
interval = 5 # Interval (milliseconds)

# [custom-message]
# move-to-channel = "You have been moved into your channel."

[raw-query]
server = ""  # TeamSpeak Server Address
port = 10011 # TeamSpeak ServerQuery(Raw) Port
user = "serveradmin" # TeamSpeak ServerQuery Username
password = "114514" # TeamSpeak ServerQuery Password

# web-query section removed since 3.0.0
```

|         Name         |      Type      | Required | Description                                                                                                                                                                                                                                                                                                              |
|:--------------------:|:--------------:|:--------:|:-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
|      additional      |     array      | Optional | The additional server configure filename part, if you have multiple server to management. You should put strings in this array.                                                                                                                                                                                          |
|        server        |     table      | Required |                                                                                                                                                                                                                                                                                                                          |
|      server-id       |    integer     | Optional | The ID of the server, which you want to get the channel. <br>If there are multiple servers running, you can get the ID via the TeamSpeak 3 Server Query. <br>Generally, the server ID is `1`.                                                                                                                            |
|      channel-id      | integer, array | Required | The ID of the channel, which you want to listen to.                                                                                                                                                                                                                                                                      |
|  privilege-group-id  |    integer     | Required | The ID of the privilege group, which will be assigned to user who joins the channel specified by `channel_id`. <br>`5` means Channel Admin Generally.                                                                                                                                                                    |
|     redis-server     |     string     | Required | Redis Server is optional if `leveldb` is specified. Redis Server Should be like `redis://[<username>][:<password>@]<hostname>[:port][/<db>]`. <br>More information about Redis URL can be found [here](https://docs.rs/redis/latest/redis/#connection-parameters).                                                       |
|       leveldb        |     string     | Required | Required if redis server is not specified                                                                                                                                                                                                                                                                                |
| track-channel-member |     string     | Optional | It will record user membership in specify database (Require `tracker` feature)                                                                                                                                                                                                                                           |
|     mute-porter      |     table      | Optional | Auto move muter user from one channel to another channel, useful in default channel.                                                                                                                                                                                                                                     |
|       monitor        |    integer     | Required | Porter monitor channel.                                                                                                                                                                                                                                                                                                  |
|        target        |    integer     | Required | Porter move user to this channel.                                                                                                                                                                                                                                                                                        |
|      whitelist       | integer, array | Optional | Porter whitelist, use database ID to identify user                                                                                                                                                                                                                                                                       |
|     permissions      |     array      | Optional | The permission you want to set to the channel.<br/>If you are listening to multiple channels, you can set the permission for each channel by just add another `permissions` section.                                                                                                                                     |
|      channel-id      |    integer     | Required | The ID of the channel, which you want to add the permission to.                                                                                                                                                                                                                                                          |
|         map          |     array      | Optional | The permission you want to set to the channel. <br/>For example, `[[125, 75], [133, 60]]` means set i_channel_needed_permission_modify_power to 75 and i_channel_needed_delete_power to 60. <br>See [Permission List](https://github.com/KunoiSayami/teamspeak-autochannel.rs/wiki/Permission-List) for more information. |
|       telegram       |     table      | Required |                                                                                                                                                                                                                                                                                                                          |
|       api-key        |     string     | Required | Telegram bot api key. If you don't use telegram, leave it blank.                                                                                                                                                                                                                                                         |
|      api-server      |     string     | Optional | Telegram bot api server, leave blank to use default server.                                                                                                                                                                                                                                                              |
|        target        |    integer     | Required | Telegram target channel (current support channel / group only).                                                                                                                                                                                                                                                          |
|      responsible     |    boolean     | Optional | Set to true if you want use bot to query current clients                                                                                                                                                                                                                                                                 |
|     allowed-chat     |     array      | Optional | Array contains chat id allow to use bot command                                                                                                                                                                                                                                                                          |
|         misc         |     table      | Required |                                                                                                                                                                                                                                                                                                                          |
|       interval       |    integer     | Optional | The interval (milliseconds) between each check.                                                                                                                                                                                                                                                                          |
|    custom-message    |     table      | Optional | The message you want to send to the user who joins the channel.                                                                                                                                                                                                                                                          |
|   move-to-channel    |     string     | Optional | The message you want to send to the user while user is moved to the their channel.                                                                                                                                                                                                                                       |
|      raw-query       |     table      | Required |                                                                                                                                                                                                                                                                                                                          |
|        server        |     string     | Required | TeamSpeak Server Address                                                                                                                                                                                                                                                                                                 |
|         port         |    integer     | Required | TeamSpeak ServerQuery(Raw) Port                                                                                                                                                                                                                                                                                          |
|         user         |     string     | Required | TeamSpeak ServerQuery Username                                                                                                                                                                                                                                                                                           |
|       password       |     string     | Required | TeamSpeak ServerQuery Password                                                                                                                                                                                                                                                                                           |

### Configuring the server

By default TeamSpeak's server rate limits server query commands from the same IP, and this tool requires a faster rate than the default limit. If you are running this on a machine that's different from the server (i.e. the `server` above is not localhost), you might need to whitelist the IP of the machine you run `teamspeak-management-tools`. You need to modify the file `query_ip_allowlist.txt` in your TeamSpeak server directory. If you for example runs the tools from `192.0.2.1`, you need to change this file to

```plain
127.0.0.1
::1
192.0.2.1
```

CIDR notation is supported here too; if you runs this tool in a different docker container (with docker's default networking) for example, you can use `172.16.0.0/12`.

## License

[![](https://www.gnu.org/graphics/agplv3-155x51.png "AGPL v3 logo")](https://www.gnu.org/licenses/agpl-3.0.txt)

Copyright (C) 2022-2024 KunoiSayami

This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or any later version.

This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.

You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.