package top.akadesk.app.ui.home

import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material.ExperimentalMaterialApi
import androidx.compose.material.LocalMinimumInteractiveComponentEnforcement
import androidx.compose.material.Slider as MaterialSlider
import androidx.compose.material.SliderDefaults as MaterialSliderDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

@OptIn(ExperimentalMaterialApi::class)
@Composable
internal fun CompactRemoteSlider(value:Float,onValueChange:(Float)->Unit,valueRange:ClosedFloatingPointRange<Float>,modifier:Modifier=Modifier) {
    CompositionLocalProvider(LocalMinimumInteractiveComponentEnforcement provides false) {
        MaterialSlider(value,onValueChange,modifier.height(28.dp),valueRange=valueRange,
            colors=MaterialSliderDefaults.colors(thumbColor=Color.White,activeTrackColor=MaterialTheme.colorScheme.primary,
                inactiveTrackColor=Color(0xFF626773)))
    }
}

@Composable
internal fun MappingKeyLabel(label:String,size:Int) {
    Text(label,modifier=Modifier.padding(2.dp),color=Color.White,fontSize=(size*.28f).coerceIn(8f,12f).sp,
        lineHeight=(size*.32f).coerceIn(9f,14f).sp,textAlign=TextAlign.Center,maxLines=2,overflow=TextOverflow.Ellipsis)
}
