typedef unsigned char uint8_t;
typedef unsigned int uint32_t;

__device__ __constant__ uint32_t SHA256_K[64] = {
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5,
    0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
    0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
    0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
    0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
    0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5,
    0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
    0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2
};

__device__ __forceinline__ uint32_t rotr(uint32_t x, uint32_t n) {
    return (x >> n) | (x << (32 - n));
}

__device__ __forceinline__ uint32_t ch(uint32_t x, uint32_t y, uint32_t z) {
    return (x & y) ^ ((~x) & z);
}

__device__ __forceinline__ uint32_t maj(uint32_t x, uint32_t y, uint32_t z) {
    return (x & y) ^ (x & z) ^ (y & z);
}

__device__ __forceinline__ uint32_t ep0(uint32_t x) {
    return rotr(x, 2) ^ rotr(x, 13) ^ rotr(x, 22);
}

__device__ __forceinline__ uint32_t ep1(uint32_t x) {
    return rotr(x, 6) ^ rotr(x, 11) ^ rotr(x, 25);
}

__device__ __forceinline__ uint32_t sig0(uint32_t x) {
    return rotr(x, 7) ^ rotr(x, 18) ^ (x >> 3);
}

__device__ __forceinline__ uint32_t sig1(uint32_t x) {
    return rotr(x, 17) ^ rotr(x, 19) ^ (x >> 10);
}

__device__ __forceinline__ uint32_t bswap32(uint32_t x) {
    return ((x & 0x000000ffu) << 24) |
           ((x & 0x0000ff00u) << 8) |
           ((x & 0x00ff0000u) >> 8) |
           ((x & 0xff000000u) >> 24);
}

__device__ __forceinline__ void store_be32(uint8_t* bytes, uint32_t value) {
    bytes[0] = (uint8_t)(value >> 24);
    bytes[1] = (uint8_t)(value >> 16);
    bytes[2] = (uint8_t)(value >> 8);
    bytes[3] = (uint8_t)value;
}

__device__ __forceinline__ uint32_t schedule_word(uint32_t* w, int round) {
    if (round < 16) {
        return w[round];
    }

    int idx = round & 15;
    uint32_t value = sig1(w[(round - 2) & 15])
                   + w[(round - 7) & 15]
                   + sig0(w[(round - 15) & 15])
                   + w[idx];
    w[idx] = value;
    return value;
}

__device__ __forceinline__ void sha256_compress_words(uint32_t* state, uint32_t* w) {
    uint32_t a = state[0];
    uint32_t b = state[1];
    uint32_t c = state[2];
    uint32_t d = state[3];
    uint32_t e = state[4];
    uint32_t f = state[5];
    uint32_t g = state[6];
    uint32_t h = state[7];

    #pragma unroll
    for (int round = 0; round < 64; ++round) {
        uint32_t wt = schedule_word(w, round);
        uint32_t t1 = h + ep1(e) + ch(e, f, g) + SHA256_K[round] + wt;
        uint32_t t2 = ep0(a) + maj(a, b, c);
        h = g;
        g = f;
        f = e;
        e = d + t1;
        d = c;
        c = b;
        b = a;
        a = t1 + t2;
    }

    state[0] += a;
    state[1] += b;
    state[2] += c;
    state[3] += d;
    state[4] += e;
    state[5] += f;
    state[6] += g;
    state[7] += h;
}

__device__ __forceinline__ bool meets_target_state(const uint32_t* state, const uint32_t* target) {
    for (int i = 7; i >= 0; --i) {
        uint32_t word = state[i];
        uint32_t target_word = target[i];
        if (word < target_word) {
            return true;
        }
        if (word > target_word) {
            return false;
        }
    }
    return true;
}

extern "C" __global__ void mine_sha256d(
    const uint32_t* midstate,
    const uint32_t* block1_prefix,
    const uint32_t* target,
    uint32_t start_nonce,
    uint32_t count,
    uint32_t max_results,
    uint32_t* found_count,
    uint32_t* found_nonces,
    uint8_t* found_hashes
) {
    uint32_t thread_index = blockIdx.x * blockDim.x + threadIdx.x;
    uint32_t stride = blockDim.x * gridDim.x;

    uint32_t base_midstate[8];
    #pragma unroll
    for (int i = 0; i < 8; ++i) {
        base_midstate[i] = midstate[i];
    }

    uint32_t block1_template[16] = {0};
    block1_template[0] = block1_prefix[0];
    block1_template[1] = block1_prefix[1];
    block1_template[2] = block1_prefix[2];
    block1_template[4] = 0x80000000u;
    block1_template[15] = 0x00000280u;

    for (uint32_t offset = thread_index; offset < count; offset += stride) {
        uint32_t nonce = start_nonce + offset;
        uint32_t first_state[8];
        uint32_t block1[16];
        uint32_t second_state[8] = {
            0x6a09e667u, 0xbb67ae85u, 0x3c6ef372u, 0xa54ff53au,
            0x510e527fu, 0x9b05688cu, 0x1f83d9abu, 0x5be0cd19u
        };

        #pragma unroll
        for (int i = 0; i < 8; ++i) {
            first_state[i] = base_midstate[i];
        }

        #pragma unroll
        for (int i = 0; i < 16; ++i) {
            block1[i] = block1_template[i];
        }
        block1[3] = bswap32(nonce);
        sha256_compress_words(first_state, block1);

        uint32_t second_block[16] = {0};
        #pragma unroll
        for (int i = 0; i < 8; ++i) {
            second_block[i] = first_state[i];
        }
        second_block[8] = 0x80000000u;
        second_block[15] = 0x00000100u;
        sha256_compress_words(second_state, second_block);

        if (meets_target_state(second_state, target)) {
            uint32_t slot = atomicAdd(found_count, 1u);
            if (slot < max_results) {
                found_nonces[slot] = nonce;
                #pragma unroll
                for (int i = 0; i < 8; ++i) {
                    store_be32(found_hashes + (slot * 32) + (i * 4), second_state[i]);
                }
            }
        }
    }
}
