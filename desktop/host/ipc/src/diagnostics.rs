/// Stable, short user messages. Raw platform errors and credentials never enter
/// the default event feed or exported report through this mapping.
pub fn presentation(code: &str) -> (&'static str, &'static str, bool) {
    match code {
        "HOST_RUNTIME_READY" | "DESKTOP_SHELL_READY" => ("已准备就绪", "", false),
        "HOST_SETTINGS_UPDATED" | "SHELL_SETTINGS_SAVED" => ("设置已保存", "", false),
        "HOST_TEMP_CODE_REFRESHED" => ("验证码已刷新", "", false),
        "REMOTE_ASSIST_REQUEST_ALLOWED" => ("已允许连接", "", false),
        "REMOTE_ASSIST_REQUEST_REJECTED" => ("已拒绝连接", "", false),
        "SESSION_PREPARING" => ("正在建立连接", "", false),
        "SESSION_STREAMING" => ("屏幕共享已开始", "", false),
        "SESSION_CLOSED" => ("连接已结束，清理完成", "", false),
        "SESSION_FAILED" => ("连接未能继续", "检查网络后重新连接", true),
        "SESSION_CONTROL_INTERRUPTED" => ("控制连接中断", "连接中断时会自动重连，请检查网络", true),
        "SESSION_CONTROL_RECOVERED" => ("控制连接已恢复", "", false),
        "SESSION_CONTROL_TIMEOUT" => ("手机连接超时，已结束共享", "检查网络后重新连接", true),
        "CONTROL_DELIVERY_FAILED" => ("控制消息发送失败", "检查网络后重新连接", true),
        "MEDIA_CONNECTION_INTERRUPTED" => ("画面连接暂时中断", "请稍候，正在等待连接恢复", true),
        "MEDIA_CONNECTION_RECOVERED" => ("画面连接已恢复", "", false),
        "MEDIA_STREAM_FAILED" => ("画面传输异常", "检查网络后重新连接", true),
        "MEDIA_STREAM_RECOVERED" => ("画面传输已恢复", "", false),
        "DISPLAY_SETTINGS_APPLIED" => ("显示设置已应用", "", false),
        "DISPLAY_RESTORE_SUCCEEDED" => ("电脑显示设置已恢复", "", false),
        "AUDIO_OUTPUT_RESTORE_SUCCEEDED" => ("电脑声音设置已恢复", "", false),
        "DISPLAY_RESTORE_FAILED" => ("电脑显示设置尚未恢复", "检查显示器连接与设置后重试", true),
        "AUDIO_OUTPUT_RESTORE_FAILED" => ("电脑声音设置尚未恢复", "检查音频设备后重试", true),
        "INPUT_RELEASE_FAILED" => ("远程按键释放失败", "在电脑上检查按键状态后重新连接", false),
        "INPUT_RELEASE_SUCCEEDED" => ("远程按键已释放", "", false),
        "SESSION_OUTPUT_RESTORE_FAILED" => {
            ("连接清理尚未完成", "检查电脑显示与音频设备后重试", true)
        }
        "IPC_CONNECTION_LOST" => ("电脑服务连接中断", "正在重新连接", true),
        "IPC_CONNECTION_RECOVERED" => ("电脑服务连接已恢复", "", false),
        "HOST_SERVICE_RESTARTED" => ("电脑服务已重新启动", "原连接已结束，请重新连接", true),
        "DIAGNOSTIC_EXPORT_FAILED" => ("诊断导出失败", "检查磁盘空间后重试", true),
        _ if code.starts_with("DISPLAY_") => ("显示设置未能完成", "刷新显示设置后重试", true),
        _ if code.starts_with("MEDIA_") || code.starts_with("AUDIO_") => {
            ("音视频连接异常", "检查网络与音视频设置后重新连接", true)
        }
        _ if code.starts_with("AUTH_") || code.starts_with("PAIRING_") => {
            ("连接验证未通过", "重新配对后连接", false)
        }
        _ if code.starts_with("INPUT_") => ("远程输入不可用", "检查电脑权限后重新连接", true),
        _ => ("操作未能完成", "请重试，仍有问题可导出诊断", true),
    }
}

pub fn category(code: &str) -> &'static str {
    match code.split('_').next().unwrap_or_default() {
        "MEDIA" | "AUDIO" => "MEDIA",
        "DISPLAY" => "DISPLAY",
        "SESSION" => "SESSION",
        "INPUT" => "INPUT",
        "PAIRING" => "PAIRING",
        "AUTH" => "AUTH",
        "IPC" => "IPC",
        "DIAGNOSTIC" => "DIAGNOSTIC",
        _ => "RUNTIME",
    }
}
