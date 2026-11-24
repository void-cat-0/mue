open-settings = 设置  [S]
settings-status =
    { $theme ->
        [light] 浅色主题
       *[dark] 深色主题
    } / 曲绘动画：{ $motion ->
        [off] 关
       *[on] 开
    } / 语言：{ $locale ->
        [en-US] 英语
       *[zh-CN] 简体中文
    }
artist-description = { $artist } / { $description }
track-level = { $bpm } 拍/分钟 / { $difficulty } { $level }
genre-progressive = 旋律渐进电子
genre-garage = 未来车库
genre-drum-bass = 氛围鼓打贝斯
genre-sunset = 日落现场
genre-night-drive = 夜间行驶
genre-quiet-currents = 静谧潮流

toast-info = 提示
toast-warning = 警告
toast-success = 成功
toast-opening-song = 正在打开 { $song }
toast-song-offscreen = 请先滚动到该曲目，再打开详情。
toast-settings-updated = 设置已更新，本次运行期间有效。
toast-dialog-acknowledged = 已确认对话框。

dialog-song-title = 正在打开 { $song }
dialog-song-body = 这个对话框属于应用，不属于歌曲库。曲绘会在它后面继续切换到详情页，对话框仍然保持打开。
    打开时不能点击下方页面。按 Esc 或点击遮罩可关闭对话框，不会同时返回歌曲库。
dialog-dismiss = 关闭
dialog-confirm = 知道了
