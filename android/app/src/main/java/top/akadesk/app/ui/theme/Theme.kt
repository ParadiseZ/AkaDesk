package top.akadesk.app.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

private val DarkColorScheme = darkColorScheme(
    primary = AkaDeskAccent,
    secondary = AkaDeskBlue,
    tertiary = AkaDeskAccent,
    background = AkaDeskSurfaceDark,
    surface = Color(0xFF0D1728),
    onPrimary = Color.White,
    onSecondary = Color.White,
    onTertiary = Color.White,
    onBackground = Color(0xFFF5F8FF),
    onSurface = Color(0xFFF5F8FF),
)

private val LightColorScheme = lightColorScheme(
    primary = AkaDeskBlue,
    secondary = AkaDeskBlueDark,
    tertiary = AkaDeskAccent,
    background = AkaDeskSurface,
    surface = Color.White,
    surfaceVariant = AkaDeskSurfaceSoft,
    onPrimary = Color.White,
    onSecondary = Color.White,
    onTertiary = Color.White,
    onBackground = AkaDeskInk,
    onSurface = AkaDeskInk,
)

@Composable
fun AkaDeskTheme(
    darkTheme: Boolean = false,
    content: @Composable () -> Unit
) {
    val colorScheme = if (darkTheme) DarkColorScheme else LightColorScheme

    MaterialTheme(
        colorScheme = colorScheme,
        typography = Typography,
        content = content
    )
}
