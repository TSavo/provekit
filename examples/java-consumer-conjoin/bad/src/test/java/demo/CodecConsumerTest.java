package demo;

import static org.junit.jupiter.api.Assertions.assertEquals;

import com.google.gson.Gson;
import java.io.ByteArrayInputStream;
import java.nio.charset.StandardCharsets;
import org.apache.commons.codec.binary.Base64;
import org.apache.commons.codec.binary.Hex;
import org.apache.commons.io.IOUtils;
import org.apache.commons.text.StringEscapeUtils;
import org.junit.jupiter.api.Test;

final class CodecConsumerTest {
    @Test
    void libraryResultsContradict() throws Exception {
        String json = new Gson().toJson(new Payload("codec-base64", 1));
        String escaped = StringEscapeUtils.escapeJson(json);
        String streamed = IOUtils.toString(new ByteArrayInputStream(
                StringEscapeUtils.unescapeJson(escaped).getBytes(StandardCharsets.UTF_8)),
                StandardCharsets.UTF_8);

        final byte[] b4 = Hex.decodeHex("2bf7cc2701fe4397b49ebeed5acc7090");

        assertEquals(json, streamed);
        assertEquals("K_fMJwH-Q5e0nr7tWsxwkA", Base64.encodeBase64String(b4));
    }

    static final class Payload {
        final String value;
        final int count;

        Payload(String value, int count) {
            this.value = value;
            this.count = count;
        }
    }
}
