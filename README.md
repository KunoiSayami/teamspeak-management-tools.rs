# TeamSpeak management tools

![License](https://img.shields.io/github/license/KunoiSayami/teamspeak-management-tools.rs?style=for-the-badge) ![GitHub release (latest SemVer)](https://img.shields.io/github/v/release/KunoiSayami/teamspeak-management-tools.rs?style=for-the-badge)

This is a simple Rust implement of TeamSpeak 3 Auto-Channel and User monitor.

## Features

You and other users can get a temporary channel automatically when you join the specified channel.

You can receive a message when user enter or left your server on [telegram](https://telegram.org/).


## Configuration

You should create a configure files in the same directory as work directory.


```toml
additional = [] # Another configure filename
[server]
server_id = 1 # Server ID
channel_id = [1, 2] # Channel ID
privilege_group_id = 5 # Channel Privilege Group ID
redis_server = "" # Redis Server Address
leveldb = "" # LevelDB database file name/path
# track_channel_member = ""

# [mute_porter]
# enable = false
# monitor = 1
# target = 1
# Should use database ID
# whitelist = []

# [[permissions]]
# channel_id = 1
# it means set i_channel_needed_permission_modify_power to 75 and i_channel_needed_delete_power to 60
# See: https://github.com/KunoiSayami/teamspeak-autochannel.rs/wiki/Permission-List for more key information
# map = [[86, 75], [133, 60]]

[telegram]
api_key = ""
target = 0
# api_server = ""

[misc]
interval = 5 # Interval (milliseconds)

# [custom_message]
# move_to_channel = "You have been moved into your channel."

[raw_query]
server = ""  # TeamSpeak Server Address
port = 10011 # TeamSpeak ServerQuery(Raw) Port
user = "serveradmin" # TeamSpeak ServerQuery Username
password = "114514" # TeamSpeak ServerQuery Password

# web_query section removed since 3.0.0
```

|         Name         |      Type      | Required | Description                                                                                                                                                                                                                                                                                                              |
|:--------------------:|:--------------:|:--------:|:-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
|      additional      |    array       | Optional | The additional server configure filename part, if you have multiple server to management. You should put strings in this array.                                                                                                                                                                                          |
|        server        |     table      | Required |                                                                                                                                                                                                                                                                                                                          |
|      server_id       |    integer     | Optional | The ID of the server, which you want to get the channel. <br>If there are multiple servers running, you can get the ID via the TeamSpeak 3 Server Query. <br>Generally, the server ID is `1`.                                                                                                                            |
|      channel_id      | integer, array | Required | The ID of the channel, which you want to listen to.                                                                                                                                                                                                                                                                      |
|  privilege_group_id  |    integer     | Required | The ID of the privilege group, which will be assigned to user who joins the channel specified by `channel_id`. <br>`5` means Channel Admin Generally.                                                                                                                                                                    |
|     redis_server     |     string     | Required | Redis Server is Required. Redis Server Should be like `redis://[<username>][:<password>@]<hostname>[:port][/<db>]`. <br>More information about Redis URL can be found [here](https://docs.rs/redis/latest/redis/#connection-parameters).                                                                                 |
| track_channel_member |     string     | Optional | It will record user membership in specify database (Require `tracker` feature)                                                                                                                                                                                                                                           |
|     mute_porter      |     table      | Optional | Auto move muter user from one channel to another channel, useful in default channel.                                                                                                                                                                                                                                     |
|       monitor        |    integer     | Required | Porter monitor channel.                                                                                                                                                                                                                                                                                                  |
|        target        |    integer     | Required | Porter move user to this channel.                                                                                                                                                                                                                                                                                        |
|      whitelist       | integer,array  | Optional | Porter whitelist, use database ID to identify user                                                                                                                                                                                                                                                                       |
|     permissions      |     array      | Optional | The permission you want to set to the channel.<br/>If you are listening to multiple channels, you can set the permission for each channel by just add another `permissions` section.                                                                                                                                     |
|      channel_id      |    integer     | Required | The ID of the channel, which you want to add the permission to.                                                                                                                                                                                                                                                          |
|         map          |     array      | Optional | The permission you want to set to the channel. <br/>For example, `[[86, 75], [133, 60]]` means set i_channel_needed_permission_modify_power to 75 and i_channel_needed_delete_power to 60. <br>See [Permission List](https://github.com/KunoiSayami/teamspeak-autochannel.rs/wiki/Permission-List) for more information. |
|       telegram       |     table      | Required |                                                                                                                                                                                                                                                                                                                          |
|       api_key        |     string     | Required | Telegram bot api key. If you don't use telegram, leave it blank.                                                                                                                                                                                                                                                         |
|        target        |    integer     | Required | Telegram target channel (current support channel only).                                                                                                                                                                                                                                                                  |
|      api_server      |     string     | Optional | Telegram bot api server, leave blank to use default server.                                                                                                                                                                                                                                                              |
|       interval       |    integer     | Optional | The interval (milliseconds) between each check.                                                                                                                                                                                                                                                                          |
|    custom_message    |     table      | Optional | The message you want to send to the user who joins the channel.                                                                                                                                                                                                                                                          |
|   move_to_channel    |     string     | Optional | The message you want to send to the user while user is moved to the their channel.                                                                                                                                                                                                                                       |
|      raw_query       |     table      | Required |                                                                                                                                                                                                                                                                                                                          |
|        server        |     string     | Required | TeamSpeak Server Address                                                                                                                                                                                                                                                                                                 |
|         port         |    integer     | Required | TeamSpeak ServerQuery(Raw) Port                                                                                                                                                                                                                                                                                          |
|         user         |     string     | Required | TeamSpeak ServerQuery Username                                                                                                                                                                                                                                                                                           |
|       password       |     string     | Required | TeamSpeak ServerQuery Password                                                                                                                                                                                                                                                                                           |


## License

[![](https://www.gnu.org/graphics/agplv3-155x51.png)](https://www.gnu.org/licenses/agpl-3.0.txt)

Copyright (C) 2022-2024 KunoiSayami

This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or any later version.

This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.

You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.