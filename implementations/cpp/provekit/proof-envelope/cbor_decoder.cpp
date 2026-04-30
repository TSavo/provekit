// SPDX-License-Identifier: Apache-2.0

#include "cbor_decoder.hpp"

#include <stdexcept>

namespace provekit::proof_envelope {

void CBORDecoder::read_head(uint8_t* major_out, uint64_t* arg_out) {
    if (pos_ >= len_) throw std::runtime_error("CBOR decode: unexpected EOF");
    uint8_t first = data_[pos_++];
    *major_out = first >> 5;
    uint8_t info = first & 0x1F;
    if (info < 24) {
        *arg_out = info;
    } else if (info == 24) {
        if (pos_ + 1 > len_) throw std::runtime_error("CBOR decode: truncated u8");
        *arg_out = data_[pos_++];
    } else if (info == 25) {
        if (pos_ + 2 > len_) throw std::runtime_error("CBOR decode: truncated u16");
        *arg_out = (uint64_t(data_[pos_]) << 8) | uint64_t(data_[pos_ + 1]);
        pos_ += 2;
    } else if (info == 26) {
        if (pos_ + 4 > len_) throw std::runtime_error("CBOR decode: truncated u32");
        *arg_out = (uint64_t(data_[pos_]) << 24) | (uint64_t(data_[pos_ + 1]) << 16)
                 | (uint64_t(data_[pos_ + 2]) << 8) | uint64_t(data_[pos_ + 3]);
        pos_ += 4;
    } else if (info == 27) {
        if (pos_ + 8 > len_) throw std::runtime_error("CBOR decode: truncated u64");
        uint64_t v = 0;
        for (int i = 0; i < 8; ++i) v = (v << 8) | uint64_t(data_[pos_ + i]);
        *arg_out = v;
        pos_ += 8;
    } else {
        throw std::runtime_error("CBOR decode: indefinite-length items not supported");
    }
}

CborValuePtr CBORDecoder::read_value() {
    uint8_t major;
    uint64_t arg;
    read_head(&major, &arg);
    auto out = std::make_shared<CborValue>();
    switch (major) {
        case 0:  // uint
            out->v = arg;
            return out;
        case 2: {  // bstr
            if (pos_ + arg > len_) throw std::runtime_error("CBOR decode: bstr exceeds remaining");
            std::vector<uint8_t> bytes(data_ + pos_, data_ + pos_ + arg);
            pos_ += arg;
            out->v = std::move(bytes);
            return out;
        }
        case 3: {  // tstr
            if (pos_ + arg > len_) throw std::runtime_error("CBOR decode: tstr exceeds remaining");
            std::string s(reinterpret_cast<const char*>(data_ + pos_), arg);
            pos_ += arg;
            out->v = std::move(s);
            return out;
        }
        case 4: {  // array
            std::vector<CborValuePtr> arr;
            arr.reserve(arg);
            for (uint64_t i = 0; i < arg; ++i) arr.push_back(read_value());
            out->v = std::move(arr);
            return out;
        }
        case 5: {  // map
            std::map<std::string, CborValuePtr> m;
            for (uint64_t i = 0; i < arg; ++i) {
                CborValuePtr key = read_value();
                if (!key->is_tstr()) throw std::runtime_error("CBOR decode: map key is not tstr");
                CborValuePtr val = read_value();
                m[key->as_tstr()] = val;
            }
            out->v = std::move(m);
            return out;
        }
        default:
            throw std::runtime_error("CBOR decode: unsupported major type " + std::to_string(major));
    }
}

CborValuePtr CBORDecoder::decode() { return read_value(); }

}  // namespace provekit::proof_envelope
