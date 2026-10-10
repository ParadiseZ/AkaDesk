package top.akadesk.app.controller

import java.math.BigInteger
import java.security.MessageDigest
import java.security.SecureRandom

internal object NumericDeviceId {
    fun display(id: String): String = if (id.all { it in '0'..'9' }) id.chunked(3).joinToString(" ") else id
    fun migrate(oldId: String): String {
        if (oldId.length == 9 && oldId.all { it in '0'..'9' }) return oldId
        val digest = MessageDigest.getInstance("SHA-256").digest(oldId.toByteArray(Charsets.UTF_8))
        return BigInteger(1, digest.copyOfRange(0, 8))
            .mod(BigInteger.valueOf(900_000_000L)).add(BigInteger.valueOf(100_000_000L)).toString()
    }

    fun create(): String = (100_000_000 + SecureRandom().nextInt(900_000_000)).toString()
}
